//! `piton agent` — launch an agentic coding tool with Piton fluency.
//!
//! "Fluency" means two things: the project's artifacts are current before the
//! agent starts, and the agent is told how to read them. Neither is something
//! the agent can work out on its own from a directory of generated Markdown.

use std::path::Path;
use std::process::Command;

use piton_compile::{BelayConfig, Framework};

use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

/// What the agent is told about the project before it begins.
fn primer(root: &Path, reference_root: &str, instruction_file: &str) -> String {
    format!(
        "This project is written in Piton, a declarative language for agent guidance.\n\
         \n\
         - Source of truth: the `.pi` files under `{}`. Compiled Markdown is derived output.\n\
         - Make lasting changes in the Piton source, then run `piton build`; never hand-edit \
           generated artifacts.\n\
         - Compiled reference documents live under `{reference_root}`. Links between them are \
           lazy: follow one when the work touches what it describes.\n\
         - Scoped guidance is written to `{instruction_file}` files placed next to the code \
           they describe.\n\
         - `piton check` validates the specbase; `piton reach` shows what the entrypoints can see.",
        root.display()
    )
}

/// The complete fluency prompt for a project, as its first configured adapter
/// lays artifacts out.
fn fluency(source_root: &Path, belay: Option<&BelayConfig>) -> String {
    let adapter = belay
        .and_then(|config| config.adapters.first())
        .and_then(|target| piton_belay::adapter::Adapter::by_target(target));

    let (reference_root, instruction_file) = match adapter {
        Some(adapter) => (adapter.reference_root, adapter.instruction_file),
        None => (".claude/reference", "CLAUDE.md"),
    };
    primer(source_root, reference_root, instruction_file)
}

pub fn run(agent: &str, print_fluency: bool, args: &[String]) -> u8 {
    let (project, _) = project::current();
    if project.config_path.is_none() {
        report::fail("no piton.config.pi found; run from a Piton project");
        return EXIT_ERRORS;
    }

    // Guidance the agent reads has to reflect the current source, so build
    // first and refuse to launch on a broken specbase.
    let root = project.root.clone();
    let source_root = project.source_root.clone();
    let belay = project
        .frameworks
        .iter()
        .find_map(|framework| match framework {
            Framework::Belay(config) => Some(config.clone()),
        });

    // Printing the prompt is read-only: nothing is built and nothing launched.
    if print_fluency {
        println!("{}", fluency(&source_root, belay.as_ref()));
        return EXIT_SUCCESS;
    }

    let compilation = project::compile_project(project);

    let mut diagnostics = compilation.diagnostics.clone();
    let mut plan = piton_belay::Plan::default();
    if let Some(config) = &belay {
        plan = piton_belay::plan(&compilation, config);
        diagnostics.extend(plan.diagnostics.iter().cloned());
    }
    diagnostics.sort();
    if report::diagnostics(
        &diagnostics,
        &|path| compilation.source_of(path).map(str::to_string),
        &root,
    ) {
        eprintln!("agent not launched; fix the errors above first");
        return EXIT_ERRORS;
    }
    if let Err(error) = piton_belay::write(&plan, &root) {
        report::fail(format!("cannot write project artifacts: {error}"));
        return EXIT_ERRORS;
    }

    let (program, launch_args) = match agent {
        "claude" => {
            let mut launch = vec![
                "--append-system-prompt".to_string(),
                fluency(&source_root, belay.as_ref()),
            ];
            launch.extend(args.iter().cloned());
            ("claude", launch)
        }
        other => {
            report::fail(format!("unknown agent `{other}`"));
            return EXIT_ERRORS;
        }
    };

    match Command::new(program)
        .args(&launch_args)
        .current_dir(&root)
        .status()
    {
        Ok(status) => {
            if status.success() {
                EXIT_SUCCESS
            } else {
                status.code().unwrap_or(1).clamp(0, 255) as u8
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            report::fail(format!(
                "`{program}` is not on PATH; install it or run it yourself with:\n  {program} --append-system-prompt '<piton primer>'"
            ));
            EXIT_ERRORS
        }
        Err(error) => {
            report::fail(format!("cannot launch `{program}`: {error}"));
            EXIT_ERRORS
        }
    }
}
