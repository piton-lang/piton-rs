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
use piton_compile::{prelude, Compilation, Framework, Project};
use piton_core::{Diagnostic, DiagnosticSink};
use piton_plugin::{ArgKind, ArgSpec, CommandSpec, Manifest, Stream};
use serde_json::{json, Value as Json};

use crate::commands::agent;
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

    fn agents(&self) -> Json {
        Json::Array(
            agent::TOOLS
                .iter()
                .map(|tool| json!({ "name": tool.name, "program": tool.program, "target": tool.target }))
                .collect(),
        )
    }

    fn fluency(&self, name: &str) -> Option<String> {
        let tool = agent::TOOLS.iter().find(|tool| tool.name == name)?;
        let belay = self.project.frameworks.iter().find_map(|framework| match framework {
            Framework::Belay(config) => Some(config.clone()),
        });
        Some(agent::fluency(&self.project.root, &self.project.source_root, tool, belay.as_ref()))
    }
}

/// The project in the working directory and the plugins it has.
pub fn load() -> (Project, DiagnosticSink, Vec<Manifest>, Vec<piton_plugin::Problem>) {
    let (project, diagnostics) = project::current();
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
    for manifest in manifests {
        let name = manifest.name.clone();
        let file = manifest.declared_in.clone();
        manifest.commands.retain(|command| {
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
pub fn run(
    project: Project,
    config_diagnostics: DiagnosticSink,
    manifests: &[Manifest],
    problems: &[piton_plugin::Problem],
    name: &str,
    matches: &ArgMatches,
) -> u8 {
    let Some((manifest, spec)) = manifests.iter().find_map(|manifest| {
        manifest
            .commands
            .iter()
            .find(|command| command.name == name)
            .map(|command| (manifest, command))
    }) else {
        report::fail(format!("no plugin adds the command `{name}`"));
        return EXIT_ERRORS;
    };

    // The configuration has to be sound before a plugin reads it.
    if report::diagnostics(&config_diagnostics, &CliServices::source_of, &project.root) {
        report::stopped("the configuration has errors");
        return EXIT_ERRORS;
    }
    report_problems(&project, manifests, problems);

    let services = Rc::new(CliServices { project });
    match piton_plugin::run_command(manifest, &spec.name, arguments(spec, matches), services) {
        Ok(code) => code,
        Err(failure) => {
            report::fail(format!("plugin `{}` failed: {failure}", manifest.name));
            EXIT_ERRORS
        }
    }
}
