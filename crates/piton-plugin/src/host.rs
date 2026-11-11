//! This module runs the command of a plugin in QuickJS.
//!
//! One runtime runs one command. A loader loads the modules of the plugin.
//! The loader knows only `@piton/plugin` and the files of the plugin. It
//! transpiles each file from TypeScript at import. The plugin gets access to
//! the machine through one host function, `__piton`. The host checks each
//! operation against the capabilities in the declaration.
//!
//! JavaScript runs only on this thread. A plugin can wait for the output of a
//! process or for a timer. Threads or deadlines collect these results and
//! give them to this thread. Then the event loop resolves the promise that
//! waits. Thus a plugin with eight processes at the same time has eight
//! promises that wait. It does not have eight threads of JavaScript.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use piton_compile::{prelude, Compilation, Project, Symbol};
use piton_core::{AnchorId, Diagnostic, Span};
use rquickjs::loader::{Loader, Resolver};
use rquickjs::{Context, Ctx, Error, Exception, Function, Module, Object, Persistent, Runtime, Value};
use serde_json::{json, Value as Json};

use crate::manifest::{is_declaration, load_order, placeholder_marker, Manifest, Source};
use crate::transpile::transpile;

/// The embedded API module that plugins import.
const API: &str = include_str!("../api/index.ts");
/// The name that plugins use to import the API module.
const API_MODULE: &str = "@piton/plugin";

/// The destination of the text that a plugin writes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// The services that the CLI gives and the plugin host cannot decide itself.
/// They are the look of output, the report of problems, and the print of a
/// slice. The host owns all other behaviour.
pub trait Services {
    fn project(&self) -> &Project;
    /// Writes text. It removes colour where the output cannot show colour.
    fn write(&self, stream: Stream, text: &str);
    /// Gives text in a named style.
    fn paint(&self, style: &str, text: &str) -> String;
    /// Reports a problem or result without a location. The kind is `error`,
    /// `warning`, `help`, `note`, `done` or `stopped`.
    fn report(&self, kind: &str, message: &str);
    /// Reports a problem with a location. It shows the source at that
    /// location.
    fn diagnostic(&self, diagnostic: Diagnostic);
    /// Reports the diagnostics of a compilation. Returns true if one or more
    /// are errors.
    fn compilation_diagnostics(&self, compilation: &Compilation) -> bool;
    /// Prints the slice of `target` from `compilation`. For each anchor, it
    /// cites the compiled location from `links`, not the source. Returns the
    /// exit code.
    fn slice(&self, compilation: &Compilation, target: &str, links: &dyn piton_emit::markdown::LinkResolver) -> u8;
    /// Asks the user to choose one of `options` on the terminal. Returns the
    /// chosen index, or None when the user cancels. The host calls it only
    /// when stdin and stdout are terminals. If the host cannot ask, there is
    /// no choice.
    fn choose(&self, _message: &str, _options: &[String]) -> Option<usize> {
        None
    }
    /// Asks the user to type an answer on the terminal. Returns the answer,
    /// or None when the user cancels. The host calls it only when stdin and
    /// stdout are terminals.
    fn ask(&self, _message: &str, _help: Option<&str>) -> Option<String> {
        None
    }
    /// Asks the user to select items of `options` on the terminal. At the
    /// start, `defaults` gives the selected items. The user must select
    /// `minimum` items or more. Returns the selected indexes, or None when
    /// the user cancels. The host calls it only when stdin and stdout are
    /// terminals.
    fn choose_many(&self, _message: &str, _options: &[String], _defaults: &[usize], _minimum: usize) -> Option<Vec<usize>> {
        None
    }
    /// Writes the files of a new project into `directory`. Each file is a
    /// path relative to `directory` and its contents. Prints each written
    /// path. If a file exists, it writes nothing. Then the error holds the
    /// files that exist.
    fn scaffold(&self, _directory: &Path, _files: &[(String, String)]) -> Result<Result<(), Vec<String>>, String> {
        Err("this host cannot start projects".to_string())
    }
    /// Tethers the dependencies of the project in `directory` and returns
    /// the result. It is `none` when the project has no dependencies or no
    /// configuration. It is `pending` when the project has dependencies and
    /// the caller sets `check`. Else it is `installed` or `failed`. A configuration
    /// with errors gives a report and `failed`.
    fn tether(&self, _directory: &Path, _check: bool) -> String {
        "failed".to_string()
    }
}

/// The reason that a command stopped without an exit code.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    /// The host could not make the runtime, or the code of the plugin did not
    /// load.
    Load(String),
    /// The code of the plugin threw an exception.
    Threw(String),
    /// The plugin waited for an event that can never occur.
    Stalled,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Load(message) => write!(f, "{message}"),
            Failure::Threw(message) => write!(f, "{message}"),
            Failure::Stalled => write!(f, "it waited for something that can never happen"),
        }
    }
}

/// The messages that the reader threads of a process send.
enum Event {
    Line(u64, Stream, String),
    Closed(u64),
}

type Waiter = Persistent<Function<'static>>;

struct Process {
    child: Child,
    lines: VecDeque<(Stream, String)>,
    /// The number of its two streams that are open.
    open: u8,
    /// Its exit code, after it exits. The inner None means a signal.
    exit: Option<Option<i32>>,
    next_waiters: VecDeque<Waiter>,
    wait_waiters: Vec<Waiter>,
}

#[derive(Default)]
struct State {
    compilations: Vec<Compilation>,
    processes: HashMap<u64, Process>,
    timers: Vec<(Instant, Waiter)>,
    temporary_files: Vec<PathBuf>,
    spaces: Vec<crate::space::Space>,
    next_id: u64,
}

struct Host {
    /// All plugins in this runtime. They are the invoked plugin and its
    /// requirements.
    plugins: Vec<Manifest>,
    services: Rc<dyn Services>,
    state: RefCell<State>,
    sender: Sender<Event>,
}

/// The request of a run to its plugin.
#[derive(Debug, Clone)]
pub enum Invocation {
    /// One of its commands, with the arguments by name.
    Command { name: String, arguments: Json },
    /// Its build. It gives the plan of the files that it wants to write.
    Build,
    /// Its inspect hook. It gives the parts of the project that are out of
    /// date.
    Inspect,
    /// Its reach hook. It gives the anchors that it reaches itself.
    Reach,
}

/// Runs a command of a plugin with its arguments. Returns the exit code
/// from the plugin. `plugins` holds all plugins of the project. The host also
/// loads the plugins that `main` requires.
pub fn run_command(
    plugins: &[Manifest],
    main: &str,
    command: &str,
    arguments: Json,
    services: Rc<dyn Services>,
) -> Result<u8, Failure> {
    let value = run(
        plugins,
        main,
        Invocation::Command { name: command.to_string(), arguments },
        services,
    )?;
    Ok(value.as_f64().unwrap_or(0.0).clamp(0.0, 255.0) as u8)
}

/// Runs the inspect hook of a plugin. Returns its findings as JSON.
pub fn run_inspect(plugins: &[Manifest], main: &str, services: Rc<dyn Services>) -> Result<Json, Failure> {
    run(plugins, main, Invocation::Inspect, services)
}

/// Runs the reach hook of a plugin. Returns the anchors that it reaches, as
/// JSON.
pub fn run_reach(plugins: &[Manifest], main: &str, services: Rc<dyn Services>) -> Result<Json, Failure> {
    run(plugins, main, Invocation::Reach, services)
}

/// Runs the build of a plugin. Returns its plan as JSON.
pub fn run_build(plugins: &[Manifest], main: &str, services: Rc<dyn Services>) -> Result<Json, Failure> {
    run(plugins, main, Invocation::Build, services)
}

/// Runs a plugin and its requirements in a separate runtime. Returns the
/// value that its handler resolved to.
pub fn run(
    plugins: &[Manifest],
    main: &str,
    invocation: Invocation,
    services: Rc<dyn Services>,
) -> Result<Json, Failure> {
    let order: Vec<Manifest> = load_order(plugins, main)
        .map_err(Failure::Load)?
        .into_iter()
        .cloned()
        .collect();
    let sources: HashMap<String, Source> = order
        .iter()
        .map(|manifest| (manifest.name.clone(), manifest.source.clone()))
        .collect();
    let (sender, receiver) = mpsc::channel();
    let host = Rc::new(Host {
        plugins: order.clone(),
        services,
        state: RefCell::new(State::default()),
        sender,
    });
    let runtime = Runtime::new().map_err(|error| Failure::Load(error.to_string()))?;
    let context = Context::full(&runtime).map_err(|error| Failure::Load(error.to_string()))?;
    runtime.set_loader(
        PluginResolver { sources: sources.clone() },
        PluginLoader { sources },
    );

    let result = start(&context, &host, &order, main, &invocation)
        .and_then(|()| event_loop(&runtime, &context, &host, &receiver));

    // In all cases, all things that the command started stop with it. The
    // host releases each handle into the runtime before the runtime.
    host.shutdown();
    context.with(|ctx| {
        let _ = ctx.globals().remove("__piton");
    });
    result
}

/// Installs the host function and starts the run. The API module takes the
/// host function before the import of plugin code. Thus a plugin can get
/// access to the host only through its own context.
fn start(
    context: &Context,
    host: &Rc<Host>,
    order: &[Manifest],
    main: &str,
    invocation: &Invocation,
) -> Result<(), Failure> {
    context.with(|ctx| {
        install(&ctx, host.clone()).map_err(|error| Failure::Load(error.to_string()))?;
        let entries: Vec<Json> = order
            .iter()
            .map(|manifest| json!([manifest.name, format!("./{}/{}", manifest.name, manifest.entry)]))
            .collect();
        let call = match invocation {
            Invocation::Command { name, arguments } => format!(
                "host.command({}, {}, {})",
                json!(main),
                json!(name),
                json!(arguments.to_string())
            ),
            Invocation::Build => format!("host.build({})", json!(main)),
            Invocation::Inspect => format!("host.inspect({})", json!(main)),
            Invocation::Reach => format!("host.reach({})", json!(main)),
        };
        let bootstrap = format!(
            "(async () => {{\n\
               const api = await import({api});\n\
               const host = api.__setup();\n\
               for (const [name, entry] of {entries}) host.register(name, (await import(entry)).default);\n\
               return {call};\n\
             }})().then(\n\
               (value) => {{ globalThis.__piton_done = {{ value: JSON.stringify(value ?? null) }}; }},\n\
               (error) => {{ globalThis.__piton_done = {{ error: error instanceof Error ? `${{error.message}}\\n${{error.stack ?? \"\"}}` : String(error) }}; }},\n\
             );\n",
            api = json!(API_MODULE),
            entries = Json::Array(entries),
        );
        Module::evaluate(ctx.clone(), "<piton>", bootstrap)
            .map(|_| ())
            .map_err(|error| Failure::Load(exception_text(&ctx, error)))
    })
}

/// Runs jobs and delivers events until the command is complete.
fn event_loop(
    runtime: &Runtime,
    context: &Context,
    host: &Rc<Host>,
    receiver: &Receiver<Event>,
) -> Result<Json, Failure> {
    loop {
        loop {
            match runtime.execute_pending_job() {
                Ok(true) => continue,
                Ok(false) => break,
                Err(exception) => {
                    let message = exception.0.with(|ctx| exception_text(&ctx, Error::Exception));
                    return Err(Failure::Threw(message));
                }
            }
        }

        if let Some(done) = context.with(|ctx| finished(&ctx)) {
            return done;
        }

        let delivered = context
            .with(|ctx| host.deliver(&ctx))
            .map_err(|error| Failure::Threw(error.to_string()))?;
        if delivered {
            continue;
        }

        if !host.has_waiters() {
            return Err(Failure::Stalled);
        }
        let timeout = host
            .next_deadline()
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(3600));
        match receiver.recv_timeout(timeout) {
            Ok(event) => {
                host.accept(event);
                // Take all other events that wait. Thus a burst of output
                // arrives in one pass.
                while let Ok(event) = receiver.try_recv() {
                    host.accept(event);
                }
            }
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {}
        }
    }
}

/// The result of the run, after its promise settles.
fn finished(ctx: &Ctx<'_>) -> Option<Result<Json, Failure>> {
    let done: Object = ctx.globals().get("__piton_done").ok()?;
    if let Ok(value) = done.get::<_, String>("value") {
        return Some(Ok(serde_json::from_str(&value).unwrap_or(Json::Null)));
    }
    let error: String = done.get("error").unwrap_or_else(|_| "the command failed".to_string());
    Some(Err(Failure::Threw(error.trim_end().to_string())))
}

/// The text of the current exception, with its stack when it has one.
fn exception_text(ctx: &Ctx<'_>, error: Error) -> String {
    if !matches!(error, Error::Exception) {
        return error.to_string();
    }
    let caught = ctx.catch();
    if let Some(exception) = caught.as_exception() {
        let message = exception.message().unwrap_or_default();
        return match exception.stack() {
            Some(stack) if !stack.trim().is_empty() => format!("{message}\n{}", stack.trim_end()),
            _ => message,
        };
    }
    caught
        .as_string()
        .and_then(|text| text.to_string().ok())
        .unwrap_or_else(|| "the plugin threw".to_string())
}

/// Puts `__piton` on the global object.
fn install<'js>(ctx: &Ctx<'js>, host: Rc<Host>) -> rquickjs::Result<()> {
    let function = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, plugin: String, operation: String, payload: String| -> rquickjs::Result<Value<'js>> {
            let payload: Json = serde_json::from_str(&payload).unwrap_or(Json::Null);
            match host.dispatch(&ctx, &plugin, &operation, &payload) {
                Ok(Reply::Json(value)) => {
                    rquickjs::String::from_str(ctx.clone(), &value.to_string()).map(|text| text.into_value())
                }
                Ok(Reply::Promise(value)) => Ok(value),
                Err(message) => Err(Exception::throw_message(&ctx, &message)),
            }
        },
    )?;
    ctx.globals().set("__piton", function)
}

/// The result of an operation. It is data now, or a promise for later.
enum Reply<'js> {
    Json(Json),
    Promise(Value<'js>),
}

fn field<'a>(payload: &'a Json, key: &str) -> &'a Json {
    payload.get(key).unwrap_or(&Json::Null)
}

fn text_field(payload: &Json, key: &str) -> Result<String, String> {
    field(payload, key)
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("`{key}` has to be text"))
}

impl Host {
    fn project(&self) -> &Project {
        self.services.project()
    }

    fn plugin(&self, name: &str) -> Result<&Manifest, String> {
        self.plugins
            .iter()
            .find(|manifest| manifest.name == name)
            .ok_or_else(|| format!("no plugin named `{name}` is loaded"))
    }

    fn refuse(manifest: &Manifest, what: &str, capability: &str) -> String {
        format!(
            "plugin `{}` may not {what}: its declaration does not list it under capabilities.{capability}",
            manifest.name
        )
    }

    fn dispatch<'js>(
        &self,
        ctx: &Ctx<'js>,
        plugin: &str,
        operation: &str,
        payload: &Json,
    ) -> Result<Reply<'js>, String> {
        let manifest = self.plugin(plugin)?;
        let reply = match operation {
            "plugin" => json!({ "name": manifest.name, "requires": manifest.requires }),
            "placeholders" => Json::Object(
                manifest
                    .placeholders
                    .iter()
                    .map(|name| (name.clone(), json!(placeholder_marker(name))))
                    .collect(),
            ),
            "relativePath" => {
                let from = text_field(payload, "from")?;
                let to = text_field(payload, "to")?;
                let directory = Path::new(&from).parent().unwrap_or(Path::new(""));
                json!(piton_emit::relative_link(directory, Path::new(&to)))
            }
            "encode" => crate::space::encode(payload)?,
            "listFiles" => {
                if !manifest.capabilities.read_files {
                    return Err(Self::refuse(manifest, "list files", "files"));
                }
                let directory = self.inside_project(&text_field(payload, "path")?)?;
                json!(list_files(&directory)?)
            }
            "space" => {
                let mut state = self.state.borrow_mut();
                let handle = field(payload, "handle").as_u64().ok_or("`handle` has to be a compilation")? as usize;
                let space = {
                    let compilation = state.compilations.get(handle).ok_or("no compilation has that handle")?;
                    crate::space::Space::new(handle, compilation, payload)?
                };
                state.spaces.push(space);
                json!(state.spaces.len() - 1)
            }
            "spaceDocument" | "spaceSections" | "spaceText" | "spaceReport" => {
                let state = self.state.borrow();
                let index = field(payload, "space").as_u64().ok_or("`space` has to be a link space")? as usize;
                let space = state.spaces.get(index).ok_or("no link space has that handle")?;
                let compilation = &state.compilations[space.compilation];
                match operation {
                    "spaceDocument" => space.document(compilation, payload)?,
                    "spaceSections" => space.sections(compilation, payload)?,
                    "spaceText" => space.text(compilation, payload)?,
                    _ => space.report(compilation),
                }
            }
            "terminal" => {
                let stdin = std::io::stdin().is_terminal();
                let stdout = std::io::stdout().is_terminal();
                let stderr = std::io::stderr().is_terminal();
                let width = (stdout || stderr)
                    .then(|| crossterm::terminal::size().ok().map(|(columns, _)| columns))
                    .flatten();
                json!({
                    "stdin": stdin,
                    "stdout": stdout,
                    "stderr": stderr,
                    "width": width,
                    "color": std::env::var_os("NO_COLOR").is_none(),
                })
            }
            "project" => {
                let project = self.project();
                json!({
                    "root": project.root,
                    "sourceRoot": project.source_root,
                    "entry": project.entry,
                    "outputDir": project.output_dir,
                    "configPath": project.config_path,
                })
            }
            "settings" => self.settings(manifest)?,
            "compile" => {
                let project = self.project().clone();
                let compilation = if field(payload, "workspace").as_bool().unwrap_or(false) {
                    Compilation::build_workspace(project, Default::default(), &[])
                } else {
                    Compilation::build(project)
                };
                let errors = compilation.diagnostics.error_count();
                let warnings = compilation.diagnostics.warning_count();
                let mut state = self.state.borrow_mut();
                state.compilations.push(compilation);
                json!({ "handle": state.compilations.len() - 1, "errors": errors, "warnings": warnings })
            }
            "diagnostics" => {
                let state = self.state.borrow();
                let compilation = self.compilation(&state, payload)?;
                json!(self.services.compilation_diagnostics(compilation))
            }
            "anchors" => {
                let state = self.state.borrow();
                let compilation = self.compilation(&state, payload)?;
                self.anchors(compilation, field(payload, "query"))
            }
            "location" => {
                let state = self.state.borrow();
                let compilation = self.compilation(&state, payload)?;
                let id = field(payload, "id").as_u64().ok_or("`id` has to be an anchor's id")?;
                let anchor = AnchorId(id as u32);
                if id as usize >= compilation.store().anchors.len() {
                    return Err("no anchor has that id".into());
                }
                let def = compilation.store().anchor(anchor);
                let span = field(payload, "property")
                    .as_str()
                    .and_then(|property| compilation.property_span(anchor, property))
                    .unwrap_or(def.name_span);
                location(&compilation.anchor_module_path(anchor), span)
            }
            "values" => {
                let state = self.state.borrow();
                let compilation = self.compilation(&state, payload)?;
                let anchor = anchor_of(compilation, payload)?;
                piton_compile::json::typed_properties(&compilation.store().anchor(anchor).properties, compilation)
            }
            "slot" => {
                let state = self.state.borrow();
                let compilation = self.compilation(&state, payload)?;
                let anchor = anchor_of(compilation, payload)?;
                let property = text_field(payload, "property")?;
                match compilation.store().anchor(anchor).slots.get(&property) {
                    Some(slot) => json!({
                        "owner": slot.owner.0,
                        "hasValue": slot.has_value,
                        "location": location(&compilation.anchor_module_path(slot.owner), slot.span),
                    }),
                    None => Json::Null,
                }
            }
            "exports" => {
                let state = self.state.borrow();
                let compilation = self.compilation(&state, payload)?;
                json!(compilation.entry_exports().into_iter().map(|anchor| anchor.0).collect::<Vec<_>>())
            }
            "stat" => {
                if !manifest.capabilities.read_files {
                    return Err(Self::refuse(manifest, "look at files", "files"));
                }
                let path = self.inside_project(&text_field(payload, "path")?)?;
                match std::fs::metadata(&path) {
                    Ok(metadata) => {
                        #[cfg(unix)]
                        let executable = {
                            use std::os::unix::fs::PermissionsExt;
                            metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
                        };
                        #[cfg(not(unix))]
                        let executable = false;
                        let kind = if metadata.is_file() {
                            "file"
                        } else if metadata.is_dir() {
                            "directory"
                        } else {
                            "other"
                        };
                        json!({ "kind": kind, "size": metadata.len(), "executable": executable })
                    }
                    Err(_) => Json::Null,
                }
            }
            "spawn" => json!(self.spawn(manifest, payload)?),
            "next" | "wait" => {
                let id = field(payload, "id").as_u64().ok_or("`id` has to be a process")?;
                let (promise, resolve) = self.promise(ctx)?;
                let mut state = self.state.borrow_mut();
                let process = state.processes.get_mut(&id).ok_or("no process has that id")?;
                if operation == "next" {
                    process.next_waiters.push_back(resolve);
                } else {
                    process.wait_waiters.push(resolve);
                }
                return Ok(Reply::Promise(promise));
            }
            "sleep" => {
                let ms = field(payload, "ms").as_f64().unwrap_or(0.0).max(0.0);
                let (promise, resolve) = self.promise(ctx)?;
                let deadline = Instant::now() + Duration::from_millis(ms as u64);
                self.state.borrow_mut().timers.push((deadline, resolve));
                return Ok(Reply::Promise(promise));
            }
            "onPath" => {
                let program = text_field(payload, "program")?;
                if !manifest.capabilities.process.contains(&program) {
                    return Err(Self::refuse(manifest, &format!("look for `{program}`"), "process"));
                }
                json!(on_path(&program))
            }
            "readFile" => {
                if !manifest.capabilities.read_files {
                    return Err(Self::refuse(manifest, "read files", "files"));
                }
                let path = self.inside_project(&text_field(payload, "path")?)?;
                json!(std::fs::read_to_string(&path)
                    .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?)
            }
            "temporaryFile" => {
                if !manifest.capabilities.temporary_files {
                    return Err(Self::refuse(manifest, "write temporary files", "temporaryFiles"));
                }
                let contents = text_field(payload, "contents")?;
                let extension = field(payload, "extension").as_str().unwrap_or("");
                let mut state = self.state.borrow_mut();
                state.next_id += 1;
                let path = std::env::temp_dir().join(format!(
                    "piton-{}-{}-{}{extension}",
                    manifest.name,
                    std::process::id(),
                    state.next_id
                ));
                std::fs::write(&path, contents)
                    .map_err(|error| format!("cannot write `{}`: {error}", path.display()))?;
                state.temporary_files.push(path.clone());
                json!(path)
            }
            "paint" => json!(self.services.paint(&text_field(payload, "style")?, &text_field(payload, "text")?)),
            "write" => {
                let text = text_field(payload, "text")?;
                match field(payload, "stream").as_str() {
                    Some("stdout") => self.services.write(Stream::Stdout, &text),
                    Some("stderr") => self.services.write(Stream::Stderr, &text),
                    Some("stderrRaw") => {
                        let mut stderr = std::io::stderr().lock();
                        let _ = stderr.write_all(text.as_bytes());
                        let _ = stderr.flush();
                    }
                    _ => return Err("`stream` is stdout, stderr or stderrRaw".into()),
                }
                Json::Null
            }
            "report" => {
                self.report(manifest, payload)?;
                Json::Null
            }
            "exec" => {
                let program = text_field(payload, "program")?;
                if !manifest.capabilities.process.contains(&program) {
                    return Err(Self::refuse(manifest, &format!("run `{program}`"), "process"));
                }
                let args: Vec<String> = field(payload, "args")
                    .as_array()
                    .map(|items| items.iter().map(|item| item.as_str().unwrap_or("").to_string()).collect())
                    .unwrap_or_default();
                let cwd = match field(payload, "cwd").as_str() {
                    Some(cwd) => self.inside_project(cwd)?,
                    None => self.project().root.clone(),
                };
                check_sizes(&program, &args, field(payload, "env"))?;
                let mut command = Command::new(&program);
                command.args(&args).current_dir(cwd);
                if let Some(env) = field(payload, "env").as_object() {
                    for (key, value) in env {
                        command.env(key, value.as_str().unwrap_or(""));
                    }
                }
                // The program has the terminal. The user sees its output and
                // prompts and gives its input. The plugin waits for it.
                let status = command.status().map_err(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        format!("`{program}` is not on PATH")
                    } else {
                        format!("cannot run `{program}`: {error}")
                    }
                })?;
                json!(status.code())
            }
            "applyEdits" => {
                let root = self.project().root.clone();
                let edits = crate::inspection::read_edits(manifest, field(payload, "edits"), &root)?;
                let written = crate::inspection::apply(&edits, &root)
                    .map_err(|error| format!("cannot write the edits: {error}"))?;
                json!(written.iter().map(|path| crate::output::slash(path)).collect::<Vec<_>>())
            }
            "choose" => {
                if !manifest.capabilities.interactive {
                    return Err(Self::refuse(manifest, "ask the user to choose", "interactive"));
                }
                let message = text_field(payload, "message")?;
                let options: Vec<String> = field(payload, "options")
                    .as_array()
                    .map(|items| items.iter().map(|item| item.as_str().unwrap_or("").to_string()).collect())
                    .unwrap_or_default();
                // A menu needs a person at a terminal to answer it.
                if options.is_empty() || !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
                    Json::Null
                } else {
                    json!(self.services.choose(&message, &options).filter(|index| *index < options.len()))
                }
            }
            "resources" => {
                let directory = text_field(payload, "path")?;
                json!(manifest.source.list(directory.trim_start_matches("./")))
            }
            "format" => {
                let source = text_field(payload, "source")?;
                let path = field(payload, "path").as_str().unwrap_or("index.pi");
                json!(piton_syntax::format::format(&source, Path::new(path)))
            }
            "ask" => {
                if !manifest.capabilities.interactive {
                    return Err(Self::refuse(manifest, "ask the user", "interactive"));
                }
                let message = text_field(payload, "message")?;
                let help = field(payload, "help").as_str();
                if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
                    Json::Null
                } else {
                    json!(self.services.ask(&message, help))
                }
            }
            "chooseMany" => {
                if !manifest.capabilities.interactive {
                    return Err(Self::refuse(manifest, "ask the user to choose", "interactive"));
                }
                let message = text_field(payload, "message")?;
                let options: Vec<String> = field(payload, "options")
                    .as_array()
                    .map(|items| items.iter().map(|item| item.as_str().unwrap_or("").to_string()).collect())
                    .unwrap_or_default();
                let defaults: Vec<usize> = field(payload, "defaults")
                    .as_array()
                    .map(|items| items.iter().filter_map(Json::as_u64).map(|index| index as usize).filter(|index| *index < options.len()).collect())
                    .unwrap_or_default();
                let minimum = field(payload, "minimum").as_u64().unwrap_or(0) as usize;
                if options.is_empty() || !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
                    Json::Null
                } else {
                    json!(self
                        .services
                        .choose_many(&message, &options, &defaults, minimum)
                        .map(|chosen| chosen.into_iter().filter(|index| *index < options.len()).collect::<Vec<_>>()))
                }
            }
            "scaffold" => {
                if !manifest.capabilities.scaffold {
                    return Err(Self::refuse(manifest, "start a project", "scaffold"));
                }
                let directory = scaffold_directory(&text_field(payload, "directory")?);
                let mut files = Vec::new();
                for file in field(payload, "files").as_array().ok_or("`files` has to be a list")? {
                    let path = text_field(file, "path")?;
                    if path.is_empty()
                        || path.starts_with('/')
                        || path.split(['/', '\\']).any(|part| part == "..")
                    {
                        return Err(format!("`{path}` has to be a path inside the new project"));
                    }
                    files.push((path, text_field(file, "contents")?));
                }
                match self.services.scaffold(&directory, &files)? {
                    Ok(()) => json!({ "existing": [] }),
                    Err(existing) => json!({ "existing": existing }),
                }
            }
            "tether" => {
                if !manifest.capabilities.scaffold {
                    return Err(Self::refuse(manifest, "tether a new project's dependencies", "scaffold"));
                }
                let directory = scaffold_directory(&text_field(payload, "directory")?);
                let check = field(payload, "check").as_bool().unwrap_or(false);
                json!(self.services.tether(&directory, check))
            }
            "resource" => {
                let path = text_field(payload, "path")?;
                let path = path.trim_start_matches("./");
                json!(manifest
                    .source
                    .read(path)
                    .ok_or_else(|| format!("plugin `{}` has no file `{path}`", manifest.name))?)
            }
            "slice" => {
                let state = self.state.borrow();
                let handle = field(payload, "handle").as_u64().ok_or("`handle` has to be a compilation")? as usize;
                let compilation = state.compilations.get(handle).ok_or("no compilation has that handle")?;
                let space = crate::space::Space::new(handle, compilation, field(payload, "layout"))?;
                let target = text_field(payload, "target")?;
                let located = crate::space::Located { space: &space, compilation };
                json!(self.services.slice(compilation, &target, &located))
            }
            other => return Err(format!("`{other}` is not an operation the host provides")),
        };
        Ok(Reply::Json(reply))
    }

    /// A new promise, and the function that resolves it. The host keeps the
    /// function until that time.
    fn promise<'js>(&self, ctx: &Ctx<'js>) -> Result<(Value<'js>, Waiter), String> {
        let (promise, resolve, _reject) = ctx.promise().map_err(|error| error.to_string())?;
        Ok((promise.into_value(), Persistent::save(ctx, resolve)))
    }

    fn compilation<'a>(&self, state: &'a State, payload: &Json) -> Result<&'a Compilation, String> {
        field(payload, "handle")
            .as_u64()
            .and_then(|handle| state.compilations.get(handle as usize))
            .ok_or_else(|| "no compilation has that handle".to_string())
    }

    /// The settings anchor of the project for this plugin, if the project
    /// lists one.
    fn settings(&self, manifest: &Manifest) -> Result<Json, String> {
        let Some(settings) = &manifest.settings else {
            return Ok(Json::Null);
        };
        let root = manifest.module_root.as_deref();
        let mut found = self
            .project()
            .plugins
            .iter()
            .filter(|entry| !is_declaration(entry) && entry.extends(root, settings));
        let Some(entry) = found.next() else {
            return Ok(Json::Null);
        };
        if let Some(second) = found.next() {
            return Err(format!(
                "piton.config.pi lists two settings for plugin `{}`, `{}` and `{}`; list one",
                manifest.name, entry.name, second.name
            ));
        }
        let mut spans = serde_json::Map::new();
        spans.insert(String::new(), location(&entry.file, entry.name_span));
        for (property, span) in &entry.spans {
            spans.insert(property.clone(), location(&entry.file, *span));
        }
        Ok(json!({
            "id": -1,
            "name": entry.name,
            "file": entry.file,
            "exported": false,
            "abstract": false,
            "keyword": "",
            "properties": entry.properties,
            "spans": spans,
            "anchors": entry.anchors.iter().map(|(name, properties)| (name.clone(), Json::Object(properties.clone()))).collect::<serde_json::Map<_, _>>(),
            "anchorLocations": entry.anchors.iter().zip(&entry.anchor_sites).map(|((name, _), (file, span))| (name.clone(), location(file, *span))).collect::<serde_json::Map<_, _>>(),
        }))
    }

    /// The anchors that a query matches, in the order of file, then name.
    fn anchors(&self, compilation: &Compilation, query: &Json) -> Json {
        let store = compilation.store();
        let base = match field(query, "extends") {
            Json::Null => None,
            extends => {
                let module = extends.get("module").and_then(Json::as_str).unwrap_or("");
                let name = extends.get("name").and_then(Json::as_str).unwrap_or("");
                match self.lookup(compilation, module, name) {
                    Some(base) => Some(base),
                    // No code loaded the module. Thus no anchor can extend it.
                    None => return json!([]),
                }
            }
        };
        let only_exported = field(query, "exported").as_bool().unwrap_or(false);
        let only_concrete = field(query, "concrete").as_bool().unwrap_or(false);
        let only_source = field(query, "withinSourceRoot").as_bool().unwrap_or(false);
        let project = self.project();
        let tethers = project.root.join(piton_compile::packages::TETHERS);

        let mut found: Vec<(PathBuf, String, Json)> = Vec::new();
        for def in &store.anchors {
            if let Some(base) = base {
                if def.id == base || !store.inherits_from(def.id, base) {
                    continue;
                }
            }
            if (only_exported && !def.exported) || (only_concrete && def.is_abstract) {
                continue;
            }
            let file = compilation.graph().get(def.module).path.clone();
            if only_source
                && (prelude::is_package_path(&file.to_string_lossy())
                    || !file.starts_with(&project.source_root)
                    || file.starts_with(&tethers))
            {
                continue;
            }
            let properties = piton_compile::json::properties_map(&def.properties, compilation);
            found.push((
                file.clone(),
                def.name.clone(),
                json!({
                    "id": def.id.0,
                    "name": def.name,
                    "file": file,
                    "exported": def.exported,
                    "abstract": def.is_abstract,
                    "keyword": def.keyword,
                    "item": def.item,
                    "properties": properties,
                }),
            ));
        }
        found.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        Json::Array(found.into_iter().map(|(_, _, record)| record).collect())
    }

    /// An exported anchor of a module. A bundled package uses its path. An
    /// installed package uses its name.
    fn lookup(&self, compilation: &Compilation, module: &str, name: &str) -> Option<AnchorId> {
        let path = if module.starts_with('@') {
            PathBuf::from(module)
        } else {
            self.project()
                .root
                .join(piton_compile::packages::TETHERS)
                .join(module)
                .join("index.pi")
        };
        let id = compilation.graph().id_for(&path)?;
        match compilation.resolution.lookup_export(id, name, &mut Default::default()) {
            Some(Symbol::Anchor(anchor)) => Some(anchor),
            _ => None,
        }
    }

    fn report(&self, manifest: &Manifest, payload: &Json) -> Result<(), String> {
        let kind = text_field(payload, "kind")?;
        let message = text_field(payload, "message")?;
        let help = field(payload, "help").as_str().map(str::to_string);
        let at = field(payload, "location");
        if matches!(kind.as_str(), "error" | "warning") && at.is_object() {
            let file = PathBuf::from(at.get("file").and_then(Json::as_str).unwrap_or(""));
            let span = Span::new(
                at.get("start").and_then(Json::as_u64).unwrap_or(0) as usize,
                at.get("end").and_then(Json::as_u64).unwrap_or(0) as usize,
            );
            let code = field(payload, "code")
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| manifest.name.clone());
            let mut diagnostic = if kind == "error" {
                Diagnostic::error(code, message, file, span)
            } else {
                Diagnostic::warning(code, message, file, span)
            };
            if let Some(help) = help {
                diagnostic = diagnostic.with_help(help);
            }
            self.services.diagnostic(diagnostic);
            return Ok(());
        }
        if !["error", "warning", "help", "note", "done", "stopped"].contains(&kind.as_str()) {
            return Err(format!("`{kind}` is not something to report"));
        }
        self.services.report(&kind, &message);
        if let Some(help) = help {
            self.services.report("help", &help);
        }
        Ok(())
    }

    /// A path that a plugin named. The path must be in the project.
    fn inside_project(&self, path: &str) -> Result<PathBuf, String> {
        let root = &self.project().root;
        let joined = piton_compile::module::normalize(&root.join(path));
        if !joined.starts_with(root) {
            return Err(format!("`{path}` is outside the project, which plugins can't read"));
        }
        Ok(joined)
    }

    fn spawn(&self, manifest: &Manifest, payload: &Json) -> Result<u64, String> {
        let program = text_field(payload, "program")?;
        if !manifest.capabilities.process.contains(&program) {
            return Err(Self::refuse(manifest, &format!("run `{program}`"), "process"));
        }
        let args: Vec<String> = field(payload, "args")
            .as_array()
            .map(|items| items.iter().map(|item| item.as_str().unwrap_or("").to_string()).collect())
            .unwrap_or_default();
        let cwd = match field(payload, "cwd").as_str() {
            Some(cwd) => self.inside_project(cwd)?,
            None => self.project().root.clone(),
        };
        check_sizes(&program, &args, field(payload, "env"))?;
        let mut command = Command::new(&program);
        command
            .args(&args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(env) = field(payload, "env").as_object() {
            for (key, value) in env {
                command.env(key, value.as_str().unwrap_or(""));
            }
        }
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                format!("`{program}` is not on PATH")
            } else {
                format!("cannot run `{program}`: {error}")
            }
        })?;

        let mut state = self.state.borrow_mut();
        state.next_id += 1;
        let id = state.next_id;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        for (stream, kind) in [
            (stdout.map(|s| Box::new(s) as Box<dyn Read + Send>), Stream::Stdout),
            (stderr.map(|s| Box::new(s) as Box<dyn Read + Send>), Stream::Stderr),
        ] {
            let sender = self.sender.clone();
            std::thread::spawn(move || {
                if let Some(stream) = stream {
                    let mut reader = BufReader::new(stream);
                    let mut buffer = Vec::new();
                    loop {
                        buffer.clear();
                        match reader.read_until(b'\n', &mut buffer) {
                            Ok(0) | Err(_) => break,
                            Ok(_) => {
                                let line = String::from_utf8_lossy(&buffer)
                                    .trim_end_matches(['\n', '\r'])
                                    .to_string();
                                if sender.send(Event::Line(id, kind, line)).is_err() {
                                    return;
                                }
                            }
                        }
                    }
                }
                let _ = sender.send(Event::Closed(id));
            });
        }
        state.processes.insert(
            id,
            Process {
                child,
                lines: VecDeque::new(),
                open: 2,
                exit: None,
                next_waiters: VecDeque::new(),
                wait_waiters: Vec::new(),
            },
        );
        Ok(id)
    }

    /// Accepts the message from a reader thread.
    fn accept(&self, event: Event) {
        let mut state = self.state.borrow_mut();
        match event {
            Event::Line(id, stream, line) => {
                if let Some(process) = state.processes.get_mut(&id) {
                    process.lines.push_back((stream, line));
                }
            }
            Event::Closed(id) => {
                if let Some(process) = state.processes.get_mut(&id) {
                    process.open = process.open.saturating_sub(1);
                    // The process closed both streams. Thus it wrote all
                    // its output. A wait for it now will not block for long.
                    if process.open == 0 {
                        process.exit = Some(process.child.wait().ok().and_then(|status| status.code()));
                    }
                }
            }
        }
    }

    /// Resolves each promise that has its answer. Returns true if it resolved
    /// one or more.
    fn deliver(&self, ctx: &Ctx<'_>) -> rquickjs::Result<bool> {
        let mut ready: Vec<(Waiter, String)> = Vec::new();
        {
            let mut state = self.state.borrow_mut();
            for process in state.processes.values_mut() {
                while !process.next_waiters.is_empty() {
                    let answer = if let Some((stream, line)) = process.lines.pop_front() {
                        let stream = if stream == Stream::Stdout { "stdout" } else { "stderr" };
                        json!({ "stream": stream, "line": line }).to_string()
                    } else if process.exit.is_some() {
                        "null".to_string()
                    } else {
                        break;
                    };
                    let waiter = process.next_waiters.pop_front().expect("checked");
                    ready.push((waiter, answer));
                }
                if let Some(exit) = process.exit {
                    for waiter in process.wait_waiters.drain(..) {
                        ready.push((waiter, json!(exit).to_string()));
                    }
                }
            }
            let now = Instant::now();
            let (due, waiting): (Vec<_>, Vec<_>) =
                std::mem::take(&mut state.timers).into_iter().partition(|(deadline, _)| *deadline <= now);
            state.timers = waiting;
            ready.extend(due.into_iter().map(|(_, waiter)| (waiter, "null".to_string())));
        }
        // The host releases the state first. The resolve step runs no
        // JavaScript yet. But a resolved function can call the host again.
        let delivered = !ready.is_empty();
        for (waiter, answer) in ready {
            let resolve = waiter.restore(ctx)?;
            resolve.call::<_, ()>((answer,))?;
        }
        Ok(delivered)
    }

    fn has_waiters(&self) -> bool {
        let state = self.state.borrow();
        !state.timers.is_empty()
            || state
                .processes
                .values()
                .any(|process| !process.next_waiters.is_empty() || !process.wait_waiters.is_empty())
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.state.borrow().timers.iter().map(|(deadline, _)| *deadline).min()
    }

    /// Stops all that the command started. Releases the values of the
    /// runtime.
    fn shutdown(&self) {
        let mut state = self.state.borrow_mut();
        for process in state.processes.values_mut() {
            if process.exit.is_none() {
                let _ = process.child.kill();
                let _ = process.child.wait();
            }
            process.next_waiters.clear();
            process.wait_waiters.clear();
        }
        state.processes.clear();
        state.timers.clear();
        for file in state.temporary_files.drain(..) {
            let _ = std::fs::remove_file(file);
        }
        state.spaces.clear();
        state.compilations.clear();
    }
}

/// The anchor that the `id` of a payload names.
fn anchor_of(compilation: &Compilation, payload: &Json) -> Result<AnchorId, String> {
    let id = field(payload, "id").as_u64().ok_or("`id` has to be an anchor's id")?;
    if id as usize >= compilation.store().anchors.len() {
        return Err("no anchor has that id".into());
    }
    Ok(AnchorId(id as u32))
}

fn location(file: &Path, span: Span) -> Json {
    json!({ "file": file, "start": span.start, "end": span.end })
}

/// All files below a directory, in order. Each path is relative to the
/// directory and uses forward slashes. Each entry tells if the file is
/// executable. It also tells if the file is a symbolic link. This function
/// never follows a symbolic link.
fn list_files(directory: &Path) -> Result<Vec<Json>, String> {
    let mut found = Vec::new();
    let mut stack = vec![directory.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current)
            .map_err(|error| format!("cannot list `{}`: {error}", current.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
            let relative = crate::output::slash(path.strip_prefix(directory).unwrap_or(&path));
            if metadata.file_type().is_symlink() {
                found.push((relative, json!({ "path": "", "executable": false, "symlink": true })));
            } else if metadata.is_dir() {
                stack.push(path);
            } else {
                #[cfg(unix)]
                let executable = {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode() & 0o111 != 0
                };
                #[cfg(not(unix))]
                let executable = false;
                found.push((relative, json!({ "path": "", "executable": executable, "symlink": false })));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(found
        .into_iter()
        .map(|(path, mut record)| {
            record["path"] = json!(path);
            record
        })
        .collect())
}

/// The largest argument or environment string that Linux accepts, in bytes.
/// The kernel calls this limit MAX_ARG_STRLEN.
const MAX_ARGUMENT: usize = 32 * 4096;

/// Refuses an argument or an environment value that is too large for the
/// system. Without this check, the start fails with "Argument list
/// too long", and the message does not tell which argument.
fn check_sizes(program: &str, args: &[String], env: &Json) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Ok(());
    }
    for (index, arg) in args.iter().enumerate() {
        if arg.len() >= MAX_ARGUMENT {
            return Err(format!(
                "cannot run `{program}`: argument {} is {} bytes, and the operating system accepts fewer than {MAX_ARGUMENT} bytes in one argument",
                index + 1,
                arg.len()
            ));
        }
    }
    for (key, value) in env.as_object().into_iter().flatten() {
        let size = key.len() + 1 + value.as_str().map_or(0, str::len);
        if size >= MAX_ARGUMENT {
            return Err(format!(
                "cannot run `{program}`: the environment variable `{key}` is {size} bytes, and the operating system accepts fewer than {MAX_ARGUMENT} bytes in one variable"
            ));
        }
    }
    Ok(())
}

/// Returns the directory of a new project, as the user wrote it. The path is
/// relative to the working directory. It is not relative to a parent
/// project.
fn scaffold_directory(directory: &str) -> PathBuf {
    if directory.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(directory)
    }
}

/// True if PATH contains `program`.
pub fn on_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|directory| {
        let candidate = directory.join(program);
        candidate.is_file()
            || (cfg!(windows)
                && ["exe", "cmd", "bat"]
                    .iter()
                    .any(|extension| candidate.with_extension(extension).is_file()))
    })
}

/// Resolves an import of a plugin. The import is `@piton/plugin`, or a
/// relative path to a file of the plugin. The resolver also tries the path
/// with `.ts`, `.js` or `/index.ts` added.
///
/// The name of each module starts with the name of its plugin, for example
/// `test/src/tags.ts`. Thus two plugins in one runtime never share a module.
/// The resolver refuses a path that goes out of the directory of the
/// plugin.
struct PluginResolver {
    sources: HashMap<String, Source>,
}

fn join(base: &str, name: &str) -> String {
    let mut parts: Vec<&str> = base.split('/').collect();
    parts.pop();
    for part in name.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.retain(|part| !part.is_empty());
    parts.join("/")
}

/// A module name, divided into its plugin and the path in that plugin.
fn split(module: &str) -> Option<(&str, &str)> {
    module.split_once('/')
}

impl Resolver for PluginResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<rquickjs::loader::ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        if name == API_MODULE {
            return Ok(name.to_string());
        }
        if !(name.starts_with("./") || name.starts_with("../")) {
            return Err(Error::new_resolving_message(
                base,
                name,
                "a plugin can import @piton/plugin and its own files, by a relative path",
            ));
        }
        let path = join(base, name);
        let Some((plugin, _)) = split(&path) else {
            return Err(Error::new_resolving_message(base, name, "it is outside the plugin"));
        };
        // Only the bootstrap of the run names a plugin. The code of a plugin
        // stays in its own directory.
        if base != "<piton>" && split(base).map(|(owner, _)| owner) != Some(plugin) {
            return Err(Error::new_resolving_message(base, name, "it is outside the plugin"));
        }
        let Some(source) = self.sources.get(plugin) else {
            return Err(Error::new_resolving_message(base, name, "it is outside the plugin"));
        };
        for candidate in [path.clone(), format!("{path}.ts"), format!("{path}.js"), format!("{path}/index.ts")] {
            let inner = &candidate[plugin.len() + 1..];
            if !inner.is_empty() && source.read(inner).is_some() {
                return Ok(candidate);
            }
        }
        Err(Error::new_resolving_message(base, name, "no such file in the plugin"))
    }
}

/// Loads a resolved module. It transpiles TypeScript during the load.
struct PluginLoader {
    sources: HashMap<String, Source>,
}

impl Loader for PluginLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<rquickjs::loader::ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, rquickjs::module::Declared>> {
        let (path, source) = if name == API_MODULE {
            ("api/index.ts".to_string(), API.to_string())
        } else {
            let (plugin, inner) =
                split(name).ok_or_else(|| Error::new_loading_message(name, "no such file in the plugin"))?;
            let source = self
                .sources
                .get(plugin)
                .and_then(|source| source.read(inner))
                .ok_or_else(|| Error::new_loading_message(name, "no such file in the plugin"))?;
            (inner.to_string(), source)
        };
        let javascript = transpile(&path, &source).map_err(|message| Error::new_loading_message(name, message))?;
        Module::declare(ctx.clone(), name, javascript)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_join_like_paths() {
        assert_eq!(join("<piton>", "./test/src/index.ts"), "test/src/index.ts");
        assert_eq!(join("test/src/index.ts", "./tags"), "test/src/tags");
        assert_eq!(join("test/src/a/b.ts", "../c"), "test/src/c");
        // A path out of the directory of the plugin goes out of its name.
        assert_eq!(split(&join("test/src/index.ts", "../../belay/src/x")), Some(("belay", "src/x")));
    }
}
