//! Running a plugin's command in QuickJS.
//!
//! One runtime runs one command. The plugin's modules are loaded through a
//! loader that only knows `@piton/plugin` and the plugin's own files, each
//! transpiled from TypeScript as it is imported. The plugin reaches the
//! machine through one host function, `__piton`, and every operation is
//! checked against the capabilities its declaration lists.
//!
//! JavaScript runs on this thread only. What a plugin waits for, a process's
//! output or a timer, is gathered by threads or deadlines and handed back
//! here, where the event loop resolves the promise that was waiting for it.
//! So a plugin running eight processes at once is eight pending promises, not
//! eight threads of JavaScript.

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

use crate::manifest::{is_declaration, Manifest, Source};
use crate::transpile::transpile;

/// The API module plugins import, embedded.
const API: &str = include_str!("../api/index.ts");
/// The name plugins import it by.
const API_MODULE: &str = "@piton/plugin";

/// Where text a plugin writes goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// What the CLI provides that the plugin host can't decide by itself: how
/// output looks, how problems are reported, and the agentic tools Piton
/// knows. Everything else is the host's own.
pub trait Services {
    fn project(&self) -> &Project;
    /// Writes text, removing colour where it isn't shown.
    fn write(&self, stream: Stream, text: &str);
    /// Text in a named style.
    fn paint(&self, style: &str, text: &str) -> String;
    /// A problem or outcome without a location: `error`, `warning`, `help`,
    /// `note`, `done` or `stopped`.
    fn report(&self, kind: &str, message: &str);
    /// A problem with a location, shown with the source it points at.
    fn diagnostic(&self, diagnostic: Diagnostic);
    /// Reports a compilation's diagnostics, returning whether any are errors.
    fn compilation_diagnostics(&self, compilation: &Compilation) -> bool;
    /// The agentic tools, as `[{name, program, target}]`.
    fn agents(&self) -> Json;
    /// The fluency prompt a tool is given, or None for an unknown tool.
    fn fluency(&self, agent: &str) -> Option<String>;
}

/// How a command ended, when it didn't return an exit code.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    /// The runtime couldn't be set up, or the plugin's code didn't load.
    Load(String),
    /// The plugin's code threw.
    Threw(String),
    /// The plugin waited for something that can never happen.
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

/// What a process's reader threads send back.
enum Event {
    Line(u64, Stream, String),
    Closed(u64),
}

type Waiter = Persistent<Function<'static>>;

struct Process {
    child: Child,
    lines: VecDeque<(Stream, String)>,
    /// How many of its two streams are still open.
    open: u8,
    /// Its exit code, once it has exited; the inner None is a signal.
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
    next_id: u64,
}

struct Host {
    manifest: Manifest,
    services: Rc<dyn Services>,
    state: RefCell<State>,
    sender: Sender<Event>,
}

/// Runs one of a plugin's commands with its arguments, returning the exit
/// code it chose.
pub fn run_command(
    manifest: &Manifest,
    command: &str,
    arguments: Json,
    services: Rc<dyn Services>,
) -> Result<u8, Failure> {
    let (sender, receiver) = mpsc::channel();
    let host = Rc::new(Host {
        manifest: manifest.clone(),
        services,
        state: RefCell::new(State::default()),
        sender,
    });
    let runtime = Runtime::new().map_err(|error| Failure::Load(error.to_string()))?;
    let context = Context::full(&runtime).map_err(|error| Failure::Load(error.to_string()))?;
    runtime.set_loader(
        PluginResolver { source: manifest.source.clone() },
        PluginLoader { source: manifest.source.clone() },
    );

    let result = start(&context, &host, &manifest.entry, command, &arguments)
        .and_then(|()| event_loop(&runtime, &context, &host, &receiver));

    // Whatever happened, nothing the command started outlives it, and every
    // handle into the runtime is let go before the runtime is.
    host.shutdown();
    context.with(|ctx| {
        let _ = ctx.globals().remove("__piton");
    });
    result
}

/// Installs the host function and starts the command.
fn start(
    context: &Context,
    host: &Rc<Host>,
    entry: &str,
    command: &str,
    arguments: &Json,
) -> Result<(), Failure> {
    context.with(|ctx| {
        install(&ctx, host.clone()).map_err(|error| Failure::Load(error.to_string()))?;
        let bootstrap = format!(
            "(async () => {{\n\
               const api = await import({api});\n\
               const plugin = (await import({entry})).default;\n\
               return api.__run(plugin, {command}, {arguments});\n\
             }})().then(\n\
               (code) => {{ globalThis.__piton_done = {{ code }}; }},\n\
               (error) => {{ globalThis.__piton_done = {{ error: error instanceof Error ? `${{error.message}}\\n${{error.stack ?? \"\"}}` : String(error) }}; }},\n\
             );\n",
            api = json!(API_MODULE),
            entry = json!(format!("./{entry}")),
            command = json!(command),
            arguments = json!(arguments.to_string()),
        );
        Module::evaluate(ctx.clone(), "<piton>", bootstrap)
            .map(|_| ())
            .map_err(|error| Failure::Load(exception_text(&ctx, error)))
    })
}

/// Runs jobs and delivers events until the command has finished.
fn event_loop(
    runtime: &Runtime,
    context: &Context,
    host: &Rc<Host>,
    receiver: &Receiver<Event>,
) -> Result<u8, Failure> {
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
                // Take everything else already waiting, so a burst of output
                // is delivered in one pass.
                while let Ok(event) = receiver.try_recv() {
                    host.accept(event);
                }
            }
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {}
        }
    }
}

/// The command's outcome, once its promise has settled.
fn finished(ctx: &Ctx<'_>) -> Option<Result<u8, Failure>> {
    let done: Object = ctx.globals().get("__piton_done").ok()?;
    if let Ok(code) = done.get::<_, f64>("code") {
        return Some(Ok(code.clamp(0.0, 255.0) as u8));
    }
    let error: String = done.get("error").unwrap_or_else(|_| "the command failed".to_string());
    Some(Err(Failure::Threw(error.trim_end().to_string())))
}

/// The text of the exception being thrown, with its stack when it has one.
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
        move |ctx: Ctx<'js>, operation: String, payload: String| -> rquickjs::Result<Value<'js>> {
            let payload: Json = serde_json::from_str(&payload).unwrap_or(Json::Null);
            match host.dispatch(&ctx, &operation, &payload) {
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

/// What an operation returns: data now, or a promise for later.
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

    fn refuse(&self, what: &str, capability: &str) -> String {
        format!(
            "plugin `{}` may not {what}: its declaration does not list it under capabilities.{capability}",
            self.manifest.name
        )
    }

    fn dispatch<'js>(&self, ctx: &Ctx<'js>, operation: &str, payload: &Json) -> Result<Reply<'js>, String> {
        let reply = match operation {
            "plugin" => json!(self.manifest.name),
            "terminal" => {
                let stdout = std::io::stdout().is_terminal();
                let stderr = std::io::stderr().is_terminal();
                let width = (stdout || stderr)
                    .then(|| crossterm::terminal::size().ok().map(|(columns, _)| columns))
                    .flatten();
                json!({
                    "stdout": stdout,
                    "stderr": stderr,
                    "width": width,
                    "color": std::env::var_os("NO_COLOR").is_none(),
                })
            }
            "project" => {
                let project = self.project();
                let belay = project.belay();
                json!({
                    "root": project.root,
                    "sourceRoot": project.source_root,
                    "entry": project.entry,
                    "outputDir": project.output_dir,
                    "configPath": project.config_path,
                    "codeRoot": belay.map(|belay| &belay.code_root),
                    "adapters": belay.map(|belay| belay.adapters.clone()).unwrap_or_default(),
                })
            }
            "settings" => self.settings()?,
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
            "spawn" => json!(self.spawn(payload)?),
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
                if !self.manifest.capabilities.process.contains(&program) {
                    return Err(self.refuse(&format!("look for `{program}`"), "process"));
                }
                json!(on_path(&program))
            }
            "readFile" => {
                if !self.manifest.capabilities.read_files {
                    return Err(self.refuse("read files", "files"));
                }
                let path = self.inside_project(&text_field(payload, "path")?)?;
                json!(std::fs::read_to_string(&path)
                    .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?)
            }
            "temporaryFile" => {
                if !self.manifest.capabilities.temporary_files {
                    return Err(self.refuse("write temporary files", "temporaryFiles"));
                }
                let contents = text_field(payload, "contents")?;
                let extension = field(payload, "extension").as_str().unwrap_or("");
                let mut state = self.state.borrow_mut();
                state.next_id += 1;
                let path = std::env::temp_dir().join(format!(
                    "piton-{}-{}-{}{extension}",
                    self.manifest.name,
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
                self.report(payload)?;
                Json::Null
            }
            "agents" => self.services.agents(),
            "fluency" => {
                let name = text_field(payload, "name")?;
                json!(self
                    .services
                    .fluency(&name)
                    .ok_or_else(|| format!("`{name}` is not an agent Piton knows"))?)
            }
            other => return Err(format!("`{other}` is not an operation the host provides")),
        };
        Ok(Reply::Json(reply))
    }

    /// A new promise, and the function that resolves it, kept until then.
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

    /// The project's settings anchor for this plugin, if it lists one.
    fn settings(&self) -> Result<Json, String> {
        let Some(settings) = &self.manifest.settings else {
            return Ok(Json::Null);
        };
        let root = self.manifest.module_root.as_deref();
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
                self.manifest.name, entry.name, second.name
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
        }))
    }

    /// The anchors a query matches, ordered by file, then name.
    fn anchors(&self, compilation: &Compilation, query: &Json) -> Json {
        let store = compilation.store();
        let base = match field(query, "extends") {
            Json::Null => None,
            extends => {
                let module = extends.get("module").and_then(Json::as_str).unwrap_or("");
                let name = extends.get("name").and_then(Json::as_str).unwrap_or("");
                match self.lookup(compilation, module, name) {
                    Some(base) => Some(base),
                    // Nothing loaded the module, so nothing can extend it.
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
                    "properties": properties,
                }),
            ));
        }
        found.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        Json::Array(found.into_iter().map(|(_, _, record)| record).collect())
    }

    /// An exported anchor of a module: a bundled package by its path, or an
    /// installed package by its name.
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

    fn report(&self, payload: &Json) -> Result<(), String> {
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
                .unwrap_or_else(|| self.manifest.name.clone());
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

    /// A path a plugin named, which has to be inside the project.
    fn inside_project(&self, path: &str) -> Result<PathBuf, String> {
        let root = &self.project().root;
        let joined = piton_compile::module::normalize(&root.join(path));
        if !joined.starts_with(root) {
            return Err(format!("`{path}` is outside the project, which plugins can't read"));
        }
        Ok(joined)
    }

    fn spawn(&self, payload: &Json) -> Result<u64, String> {
        let program = text_field(payload, "program")?;
        if !self.manifest.capabilities.process.contains(&program) {
            return Err(self.refuse(&format!("run `{program}`"), "process"));
        }
        let args: Vec<String> = field(payload, "args")
            .as_array()
            .map(|items| items.iter().map(|item| item.as_str().unwrap_or("").to_string()).collect())
            .unwrap_or_default();
        let cwd = match field(payload, "cwd").as_str() {
            Some(cwd) => self.inside_project(cwd)?,
            None => self.project().root.clone(),
        };
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

    /// Takes in what a reader thread sent.
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
                    // Both streams are closed, so the process is done
                    // writing; waiting for it now won't block for long.
                    if process.open == 0 {
                        process.exit = Some(process.child.wait().ok().and_then(|status| status.code()));
                    }
                }
            }
        }
    }

    /// Resolves every promise whose answer has arrived, returning whether it
    /// resolved any.
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
        // The state is released first: resolving runs no JavaScript yet, but
        // a resolved function may call back into the host.
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

    /// Stops what the command started and lets go of the runtime's values.
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
        state.compilations.clear();
    }
}

fn location(file: &Path, span: Span) -> Json {
    json!({ "file": file, "start": span.start, "end": span.end })
}

/// Whether `program` can be found on PATH.
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

/// Resolves what a plugin imports: `@piton/plugin`, or a relative path to
/// one of its own files, with `.ts`, `.js` or `/index.ts` tried after it.
struct PluginResolver {
    source: Source,
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
        if base != "<piton>" && path.starts_with("..") {
            return Err(Error::new_resolving_message(base, name, "it is outside the plugin"));
        }
        for candidate in [path.clone(), format!("{path}.ts"), format!("{path}.js"), format!("{path}/index.ts")] {
            if self.source.read(&candidate).is_some() {
                return Ok(candidate);
            }
        }
        Err(Error::new_resolving_message(base, name, "no such file in the plugin"))
    }
}

/// Loads a resolved module, transpiling TypeScript as it goes.
struct PluginLoader {
    source: Source,
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
            let source = self
                .source
                .read(name)
                .ok_or_else(|| Error::new_loading_message(name, "no such file in the plugin"))?;
            (name.to_string(), source)
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
        assert_eq!(join("<piton>", "./src/index.ts"), "src/index.ts");
        assert_eq!(join("src/index.ts", "./tags"), "src/tags");
        assert_eq!(join("src/a/b.ts", "../c"), "src/c");
    }
}
