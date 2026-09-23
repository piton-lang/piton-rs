//! `piton agent` — launch an agentic coding tool with Piton fluency.
//!
//! "Fluency" is what the agent is told before it begins: a short primer about
//! this project, followed by the fluency prompt -- the output of the
//! GenerateFluencyPrompt skill, which lives in `FLUENCY_PROMPT.md` at the
//! project root. None of it is something the agent could work out on its own
//! from a directory of generated Markdown.

use std::path::Path;
use std::process::Command;

use piton_compile::{BelayConfig, Framework};

use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

/// The file the GenerateFluencyPrompt skill writes, at the project root.
const FLUENCY_FILE: &str = "FLUENCY_PROMPT.md";

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

/// The fluency prompt the compiler was built with: a copy of the repository's
/// `FLUENCY_PROMPT.md`, used when a project has none of its own.
const EMBEDDED_FLUENCY: &str = include_str!("../fluency.md");

/// The fluency prompt for a project: its own `FLUENCY_PROMPT.md` when it has
/// one, since that is the one generated from the specification it is using,
/// and the embedded copy otherwise.
fn reference(root: &Path) -> String {
    std::fs::read_to_string(root.join(FLUENCY_FILE))
        .ok()
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| EMBEDDED_FLUENCY.to_string())
}

/// The complete fluency prompt for a project, as its first configured adapter
/// lays artifacts out.
fn fluency(root: &Path, source_root: &Path, belay: Option<&BelayConfig>) -> String {
    let adapter = belay
        .and_then(|config| config.adapters.first())
        .and_then(|target| piton_belay::adapter::Adapter::by_target(target));

    let (reference_root, instruction_file) = match adapter {
        Some(adapter) => (adapter.reference_root, adapter.instruction_file),
        None => (".claude/reference", "CLAUDE.md"),
    };
    format!(
        "{}\n\n{}",
        primer(source_root, reference_root, instruction_file),
        reference(root).trim_end()
    )
}

pub fn run(agent: &str, print_fluency: bool, args: &[String]) -> u8 {
    // Without a configuration the working directory is the project, which is
    // all the prompt needs: where the source is, and where FLUENCY_PROMPT.md
    // would be.
    let (project, _) = project::current();
    let root = project.root.clone();
    let belay = project
        .frameworks
        .iter()
        .find_map(|framework| match framework {
            Framework::Belay(config) => Some(config.clone()),
        });
    let prompt = fluency(&root, &project.source_root, belay.as_ref());

    // Printing the prompt is read-only: nothing is built and nothing launched.
    if print_fluency {
        println!("{prompt}");
        return EXIT_SUCCESS;
    }

    let (program, launch_args) = match agent {
        "claude" => {
            let mut launch = vec!["--append-system-prompt".to_string(), prompt];
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
                "`{program}` is not on PATH; install it or print the prompt with `piton agent --print-fluency`"
            ));
            EXIT_ERRORS
        }
        Err(error) => {
            report::fail(format!("cannot launch `{program}`: {error}"));
            EXIT_ERRORS
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_copy_is_the_fluency_prompt() {
        assert!(EMBEDDED_FLUENCY.starts_with("# Piton Fluency"));
    }

    #[test]
    fn a_project_fluency_prompt_is_preferred() {
        let directory = std::env::temp_dir().join(format!(
            "piton-agent-fluency-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("temp dir");
        assert_eq!(reference(&directory), EMBEDDED_FLUENCY);
        std::fs::write(directory.join(FLUENCY_FILE), "# Project Fluency\n").expect("write");
        assert_eq!(reference(&directory), "# Project Fluency\n");
        let _ = std::fs::remove_dir_all(&directory);
    }
}
