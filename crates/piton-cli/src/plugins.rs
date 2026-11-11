//! Plugin commands in the CLI.
//!
//! A plugin's commands are added to the CLI from its declaration: their names,
//! help and arguments are Piton, so `piton --help` lists them and clap parses
//! their arguments without running any of the plugin's code. Only invoking one
//! starts QuickJS. This module is also what the plugin host asks for the
//! things that belong to the CLI: how output looks, how problems are reported,
//! and the agentic tools `piton agent` knows.

use std::path::Path;
use std::rc::Rc;

use clap::{Arg, ArgAction, ArgMatches};
use piton_compile::{prelude, Compilation, Project};
use piton_core::{Diagnostic, DiagnosticSink};
use piton_plugin::{ArgKind, ArgSpec, CommandSpec, Manifest, Stream};
use serde_json::{json, Value as Json};

use crate::style::{self, paint};
use crate::{project, report, EXIT_ERRORS};

/// clap wants names that live as long as the program; a CLI parses once, so
/// leaking the few a plugin declares is the simple way to give it them.
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

/// The arguments clap parsed, by name, as the plugin receives them: a flag is
/// a boolean, a multiple argument a list, anything not given null.
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

/// What the plugin host asks the CLI for.
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

}

/// The project in the working directory and the plugins it has.
/// The project is the one the working directory is inside, so a plugin's
/// command works from any directory of the project, as `piton slice` does.
pub fn load() -> (Project, DiagnosticSink, Vec<Manifest>, Vec<piton_plugin::Problem>) {
    let (project, diagnostics) = project::enclosing();
    let (manifests, problems) = piton_plugin::discover(&project);
    (project, diagnostics, manifests, problems)
}

/// Leaves out plugin commands named like one of the compiler's own, which
/// always wins, and reports each.
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

/// Reports what is wrong with the project's plugins: declarations that
/// aren't well-formed, and entries under `plugins` nothing reads.
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

/// Runs a plugin command clap has parsed.
/// Adds every plugin command to the CLI: ungrouped ones at the top level,
/// grouped ones under a command named for their group, which lists them.
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

/// The plugin command clap matched, if it matched one: the plugin, the
/// command, and its arguments.
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

pub fn run(
    project: Project,
    config_diagnostics: DiagnosticSink,
    manifests: &[Manifest],
    problems: &[piton_plugin::Problem],
    manifest: &Manifest,
    spec: &CommandSpec,
    matches: &ArgMatches,
) -> u8 {

    // The configuration has to be sound before a plugin reads it, except for
    // a plugin that inspects projects: repairing the configuration may be
    // what its commands are for.
    if !manifest.inspect && report::diagnostics(&config_diagnostics, &CliServices::source_of, &project.root) {
        report::stopped("the configuration has errors");
        return EXIT_ERRORS;
    }
    report_problems(&project, manifests, problems);

    let services = Rc::new(CliServices { project });
    match piton_plugin::run_command(manifests, &manifest.name, &spec.name, arguments(spec, matches), services) {
        Ok(code) => code,
        Err(failure) => {
            report::fail(format!("plugin `{}` failed: {failure}", manifest.name));
            EXIT_ERRORS
        }
    }
}

/// Runs the build of every plugin the project turned on, returning what each
/// planned, or the failures. Nothing is written.
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
