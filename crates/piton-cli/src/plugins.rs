//! Plugin commands in the CLI.
//!
//! The CLI adds the commands of a plugin from its declaration. Their names,
//! help and arguments are Piton. Thus `piton --help` lists them. Also, clap
//! parses their arguments and runs no plugin code. Only a call to a command
//! starts QuickJS. The plugin host also asks this module for the parts that
//! belong to the CLI. These parts are the output style, the problem reports,
//! and the agentic tools that `piton agent` knows.

use std::path::Path;
use std::rc::Rc;

use clap::{Arg, ArgAction, ArgMatches};
use piton_compile::{prelude, Compilation, Project};
use piton_core::{Diagnostic, DiagnosticSink};
use piton_plugin::{ArgKind, ArgSpec, CommandSpec, Manifest, Stream};
use serde_json::{json, Value as Json};

use crate::style::{self, paint};
use crate::{project, report, EXIT_ERRORS};

/// clap needs names that live as long as the program. A CLI parses only one time.
/// Thus the simple method is to leak the few names that a plugin declares.
fn leak(text: &str) -> &'static str {
    Box::leak(text.to_string().into_boxed_str())
}

/// The clap command for one plugin command.
pub fn clap_command(manifest: &Manifest, spec: &CommandSpec) -> clap::Command {
    let mut command = clap::Command::new(leak(&spec.name));
    let about = spec
        .about
        .clone()
        .unwrap_or_else(|| format!("Added by the {} plugin", manifest.name));
    command = command.about(about.clone());
    if let Some(help) = &spec.help {
        command = command.long_about(format!("{about}\n\n{help}"));
    }
    for argument in &spec.arguments {
        command = command.arg(clap_arg(argument));
    }
    command
}

fn clap_arg(spec: &ArgSpec) -> Arg {
    let mut arg = Arg::new(leak(&spec.name));
    if let Some(help) = &spec.help {
        arg = arg.help(help.clone());
    }
    if spec.kind != ArgKind::Flag {
        arg = arg.value_name(leak(&piton_core::kebab_case(&spec.name).to_uppercase()));
    }
    if !spec.positional {
        arg = arg.long(leak(&spec.name));
        if let Some(short) = spec.short {
            arg = arg.short(short);
        }
    }
    arg = match spec.kind {
        ArgKind::Flag => arg.action(ArgAction::SetTrue),
        _ if spec.multiple => arg.action(ArgAction::Append).num_args(1..),
        _ if spec.optional_value => arg.action(ArgAction::Set).num_args(0..=1).default_missing_value(""),
        _ => arg.action(ArgAction::Set).num_args(1),
    };
    if !spec.choices.is_empty() {
        let choices: Vec<&'static str> = spec.choices.iter().map(|choice| leak(choice)).collect();
        arg = arg.value_parser(clap::builder::PossibleValuesParser::new(choices));
    } else if spec.kind == ArgKind::Number {
        let minimum = spec.minimum;
        arg = arg.value_parser(move |text: &str| -> Result<String, String> {
            let number: f64 = text.parse().map_err(|_| format!("`{text}` is not a number"))?;
            if let Some(minimum) = minimum {
                if number < minimum {
                    return Err(format!("it has to be at least {minimum}"));
                }
            }
            Ok(text.to_string())
        });
    }
    if let Some(default) = &spec.default {
        arg = arg.default_value(leak(default));
    }
    if spec.trailing {
        arg = arg.trailing_var_arg(true).allow_hyphen_values(true).num_args(0..);
    }
    if !spec.conflicts.is_empty() {
        let conflicts: Vec<&'static str> = spec.conflicts.iter().map(|name| leak(name)).collect();
        arg = arg.conflicts_with_all(conflicts);
    }
    arg
}

/// The arguments that clap parsed, by name, in the form that goes to the plugin.
/// A flag is a boolean. A multiple argument is a list. An argument that is not there is
/// null.
fn arguments(spec: &CommandSpec, matches: &ArgMatches) -> Json {
    let mut out = serde_json::Map::new();
    for argument in &spec.arguments {
        let name = argument.name.as_str();
        let convert = |text: &String| -> Json {
            if argument.kind == ArgKind::Number {
                text.parse::<f64>().ok().map(|number| json!(number)).unwrap_or(Json::Null)
            } else {
                json!(text)
            }
        };
        let value = match argument.kind {
            ArgKind::Flag => json!(matches.get_flag(name)),
            _ if argument.multiple => match matches.get_many::<String>(name) {
                Some(values) => Json::Array(values.map(convert).collect()),
                None => Json::Null,
            },
            _ => matches.get_one::<String>(name).map(convert).unwrap_or(Json::Null),
        };
        out.insert(argument.name.clone(), value);
    }
    Json::Object(out)
}

/// The services that the plugin host asks from the CLI.
pub struct CliServices {
    pub project: Project,
}

impl CliServices {
    fn source_of(path: &Path) -> Option<String> {
        let text = path.to_string_lossy();
        prelude::package_source(&text)
            .map(str::to_string)
            .or_else(|| std::fs::read_to_string(path).ok())
    }
}

impl piton_plugin::Services for CliServices {
    fn project(&self) -> &Project {
        &self.project
    }

    fn write(&self, stream: Stream, text: &str) {
        match stream {
            Stream::Stdout => {
                anstream::print!("{text}");
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
            Stream::Stderr => anstream::eprint!("{text}"),
        }
    }

    fn paint(&self, name: &str, text: &str) -> String {
        let chosen = match name {
            "error" => style::ERROR,
            "warning" => style::WARNING,
            "success" => style::SUCCESS,
            "help" => style::HELP,
            "note" => style::NOTE,
            "name" => style::NAME,
            "heading" => style::HEADING,
            "dim" => style::DIM,
            _ => return text.to_string(),
        };
        paint(chosen, text)
    }

    fn report(&self, kind: &str, message: &str) {
        match kind {
            "error" => report::fail(message),
            "warning" => report::warn(message),
            "help" => report::help(message),
            "note" => report::note(message),
            "done" => report::done(message),
            "stopped" => report::stopped(message),
            _ => {}
        }
    }

    fn diagnostic(&self, diagnostic: Diagnostic) {
        let mut sink = DiagnosticSink::new();
        sink.push(diagnostic);
        report::diagnostics(&sink, &Self::source_of, &self.project.root);
    }

    fn compilation_diagnostics(&self, compilation: &Compilation) -> bool {
        let errors = report::diagnostics(
            &compilation.diagnostics,
            &|path| compilation.source_of(path).map(str::to_string),
            &self.project.root,
        );
        if errors {
            report::summary(&compilation.diagnostics);
        }
        errors
    }

    fn choose(&self, message: &str, options: &[String]) -> Option<usize> {
        let _ = std::io::Write::flush(&mut std::io::stdout());
        inquire::Select::new(message, options.to_vec())
            .raw_prompt()
            .ok()
            .map(|chosen| chosen.index)
    }

    fn slice(&self, compilation: &Compilation, target: &str, links: &dyn piton_emit::markdown::LinkResolver) -> u8 {
        crate::commands::slice::cite(compilation, target, links)
    }

    fn ask(&self, message: &str, help: Option<&str>) -> Option<String> {
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let mut prompt = inquire::Text::new(message);
        if let Some(help) = help {
            prompt = prompt.with_help_message(help);
        }
        prompt.prompt().ok()
    }

    fn choose_many(&self, message: &str, options: &[String], defaults: &[usize], minimum: usize) -> Option<Vec<usize>> {
        let _ = std::io::Write::flush(&mut std::io::stdout());
        inquire::MultiSelect::new(message, options.to_vec())
            .with_default(defaults)
            .with_help_message("space to toggle, enter to accept")
            .with_validator(move |chosen: &[inquire::list_option::ListOption<&String>]| {
                Ok(if chosen.len() < minimum {
                    inquire::validator::Validation::Invalid(
                        format!("choose at least {minimum}").into(),
                    )
                } else {
                    inquire::validator::Validation::Valid
                })
            })
            .raw_prompt()
            .ok()
            .map(|chosen| chosen.into_iter().map(|option| option.index).collect())
    }

    fn scaffold(&self, directory: &Path, files: &[(String, String)]) -> Result<Result<(), Vec<String>>, String> {
        let existing: Vec<String> = files
            .iter()
            .map(|(path, _)| path.clone())
            .filter(|path| directory.join(path).exists())
            .collect();
        if !existing.is_empty() {
            return Ok(Err(existing));
        }
        for (path, contents) in files {
            let destination = directory.join(path);
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("cannot create `{}`: {error}", parent.display()))?;
            }
            std::fs::write(&destination, contents)
                .map_err(|error| format!("cannot write `{}`: {error}", destination.display()))?;
            let shown = if directory == Path::new(".") {
                path.clone()
            } else {
                destination.display().to_string()
            };
            anstream::println!("{}", report::added(shown));
        }
        Ok(Ok(()))
    }

    fn tether(&self, directory: &Path, check: bool) -> String {
        let config = directory.join("piton.config.pi");
        if !config.is_file() {
            return "none".to_string();
        }
        let (configured, diagnostics) = project::from_config(Some(&config));
        if crate::commands::tether::report_config_errors(&configured, &diagnostics) {
            return "failed".to_string();
        }
        if configured.dependencies.is_empty() {
            "none".to_string()
        } else if check {
            "pending".to_string()
        } else if crate::commands::tether::install_all(&configured) == crate::EXIT_SUCCESS {
            "installed".to_string()
        } else {
            "failed".to_string()
        }
    }

}

/// The project in the current directory, and its plugins.
/// The project is the one that contains the current directory. Thus a plugin
/// command works from all directories of the project, as `piton slice` does.
pub fn load() -> (Project, DiagnosticSink, Vec<Manifest>, Vec<piton_plugin::Problem>) {
    let (project, diagnostics) = project::enclosing();
    let (manifests, problems) = piton_plugin::discover(&project);
    (project, diagnostics, manifests, problems)
}

/// Removes a plugin command with the same name as a command of the compiler,
/// and reports it. The command of the compiler always wins.
pub fn withhold_builtins(
    cli: &clap::Command,
    manifests: &mut [Manifest],
    problems: &mut Vec<piton_plugin::Problem>,
) {
    let builtin: Vec<String> = cli
        .get_subcommands()
        .map(|command| command.get_name().to_string())
        .chain(["help".to_string()])
        .collect();
    let ungrouped: Vec<String> = manifests
        .iter()
        .flat_map(|manifest| manifest.commands.iter())
        .filter(|command| command.group.is_none())
        .map(|command| command.name.clone())
        .collect();
    for manifest in manifests {
        let name = manifest.name.clone();
        let file = manifest.declared_in.clone();
        manifest.commands.retain(|command| {
            if let Some(group) = &command.group {
                if builtin.contains(group) || ungrouped.contains(group) {
                    problems.push(piton_plugin::Problem {
                        file: file.clone(),
                        message: format!(
                            "plugin `{name}` groups `{}` under `{group}`, which is already a command",
                            command.name
                        ),
                    });
                    return false;
                }
                return true;
            }
            if builtin.contains(&command.name) {
                problems.push(piton_plugin::Problem {
                    file: file.clone(),
                    message: format!(
                        "plugin `{name}` adds the command `{}`, which piton already has",
                        command.name
                    ),
                });
                false
            } else {
                true
            }
        });
    }
}

/// Reports the problems with the plugins of the project. These are declarations
/// with a bad form, and entries under `plugins` that no plugin reads.
pub fn report_problems(project: &Project, manifests: &[Manifest], problems: &[piton_plugin::Problem]) {
    for problem in problems {
        report::warn(format!("{}: {}", problem.file.display(), problem.message));
    }
    let mut sink = DiagnosticSink::new();
    for entry in piton_plugin::unclaimed(project, manifests) {
        sink.push(
            Diagnostic::warning(
                "unknown-plugin",
                format!("`{}` is not a plugin, or the settings of one", entry.name),
                &entry.file,
                entry.name_span,
            )
            .with_help("list a piton-plugin declaration, or the settings anchor of a plugin, like a test-config".to_string()),
        );
    }
    report::diagnostics(&sink, &CliServices::source_of, &project.root);
}

/// Adds all plugin commands to the CLI. A command without a group goes at the
/// top level. A command with a group goes under a command with the name of
/// the group. That command lists them.
pub fn add_commands(mut cli: clap::Command, manifests: &[Manifest]) -> clap::Command {
    let mut groups: Vec<(String, clap::Command)> = Vec::new();
    for manifest in manifests {
        for spec in &manifest.commands {
            let command = clap_command(manifest, spec);
            match &spec.group {
                None => cli = cli.subcommand(command),
                Some(group) => match groups.iter_mut().find(|(name, _)| name == group) {
                    Some((_, existing)) => {
                        *existing = std::mem::take(existing).subcommand(command);
                    }
                    None => groups.push((
                        group.clone(),
                        clap::Command::new(leak(group))
                            .about(format!("Commands from the {} plugin", manifest.name))
                            .subcommand_required(true)
                            .arg_required_else_help(true)
                            .subcommand(command),
                    )),
                },
            }
        }
    }
    for (_, group) in groups {
        cli = cli.subcommand(group);
    }
    cli
}

/// The plugin command that clap matched, if there is one. It gives the plugin,
/// the command, and its arguments.
pub fn matched<'a>(
    manifests: &'a [Manifest],
    matches: &'a ArgMatches,
) -> Option<(&'a Manifest, &'a CommandSpec, &'a ArgMatches)> {
    let (name, sub) = matches.subcommand()?;
    for manifest in manifests {
        for spec in &manifest.commands {
            match &spec.group {
                None if spec.name == name => return Some((manifest, spec, sub)),
                Some(group) if group == name => {
                    if let Some((inner, args)) = sub.subcommand() {
                        if inner == spec.name {
                            return Some((manifest, spec, args));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Runs a plugin command that clap parsed.
pub fn run(
    project: Project,
    config_diagnostics: DiagnosticSink,
    manifests: &[Manifest],
    problems: &[piton_plugin::Problem],
    manifest: &Manifest,
    spec: &CommandSpec,
    matches: &ArgMatches,
) -> u8 {

    // The configuration must have no errors before a plugin reads it. There
    // are two exceptions. The commands of a plugin with an inspect hook can
    // repair the configuration. A standalone command, such as `piton init`,
    // does not use the project.
    if !manifest.inspect && !spec.standalone && report::diagnostics(&config_diagnostics, &CliServices::source_of, &project.root) {
        report::stopped("the configuration has errors");
        return EXIT_ERRORS;
    }
    if !spec.standalone {
        report_problems(&project, manifests, problems);
    }

    let services = Rc::new(CliServices { project });
    match piton_plugin::run_command(manifests, &manifest.name, &spec.name, arguments(spec, matches), services) {
        Ok(code) => code,
        Err(failure) => {
            report::fail(format!("plugin `{}` failed: {failure}", manifest.name));
            EXIT_ERRORS
        }
    }
}

/// Runs the build of each plugin that the project enables. Returns the plan of
/// each plugin, or the failures. It writes no files.
pub fn plan_builds(
    project: &Project,
    manifests: &[Manifest],
) -> (Vec<piton_plugin::plan::PluginPlan>, Vec<String>) {
    let mut plans = Vec::new();
    let mut failures = Vec::new();
    for manifest in piton_plugin::builders(project, manifests) {
        let services = Rc::new(CliServices { project: project.clone() });
        match piton_plugin::run_build(manifests, &manifest.name, services) {
            Ok(plan) => match piton_plugin::plan::read(manifest, &plan, &project.root) {
                Ok(plan) => plans.push(plan),
                Err(message) => failures.push(format!("plugin `{}` planned something it can't build: {message}", manifest.name)),
            },
            Err(failure) => failures.push(format!("plugin `{}` failed: {failure}", manifest.name)),
        }
    }
    (plans, failures)
}
