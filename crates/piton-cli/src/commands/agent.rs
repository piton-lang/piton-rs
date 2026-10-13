//! `piton agent` — launch an agentic coding tool (Claude Code, Codex, OpenCode
//! or Muse Code) with Piton fluency.
//!
//! "Fluency" is what the agent is told before it begins: a short primer about
//! this project, followed by the fluency prompt -- the output of the
//! GenerateFluencyPrompt skill, which lives in `FLUENCY_PROMPT.md` at the
//! project root. None of it is something the agent could work out on its own
//! from a directory of generated Markdown.

use std::path::{Path, PathBuf};
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
         - `piton check` validates the specbase; `piton reach` shows what the entrypoints can see; \
           `piton slice file.pi#Anchor.property` prints just the spec one thing depends on.",
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

/// An agentic coding tool `piton agent` can launch.
struct Tool {
    /// The name given on the command line.
    name: &'static str,
    /// The program run.
    program: &'static str,
    /// The Belay target whose artifacts the tool reads.
    target: &'static str,
    /// Subcommands that reject the options carrying the prompt. Passed
    /// through as given, the tool runs without fluency.
    plain_subcommands: &'static [&'static str],
    /// The one subcommand that takes the prompt options after its name rather
    /// than before.
    prompted_subcommand: Option<&'static str>,
}

const TOOLS: &[Tool] = &[
    Tool {
        name: "claude",
        program: "claude",
        target: "claude-code",
        plain_subcommands: &[],
        prompted_subcommand: None,
    },
    Tool {
        name: "codex",
        program: "codex",
        target: "codex",
        plain_subcommands: &[],
        prompted_subcommand: None,
    },
    Tool {
        name: "opencode",
        program: "opencode",
        target: "opencode",
        plain_subcommands: &[
            "upgrade", "update", "uninstall", "acp", "api", "debug", "auth", "mcp", "plugin",
            "models", "stats", "mini", "session", "service",
        ],
        prompted_subcommand: Some("run"),
    },
    Tool {
        name: "muse",
        program: "muse",
        target: "muse",
        plain_subcommands: &[
            "resume", "config", "export", "trace", "model-profile", "skills", "voice", "plugins",
            "sandbox", "schema", "serve", "session-message", "mcp", "auth", "login", "logout",
            "init",
        ],
        prompted_subcommand: Some("exec"),
    },
];

/// Where a tool finds the project's compiled guidance: the reference root and
/// the scoped instruction file.
///
/// With the tool's own adapter configured, its instruction file is the one
/// written for it. Its references are its own when a build wrote them; in a
/// shared project only the first adapter writes a reference tree, and every
/// tool links into that one.
fn layout(root: &Path, tool: &Tool, belay: Option<&BelayConfig>) -> (&'static str, &'static str) {
    let adapter = |target: &str| piton_belay::adapter::Adapter::by_target(target);
    let own = adapter(tool.target).expect("every tool has an adapter");
    let configured = belay.map(|config| config.adapters.as_slice()).unwrap_or_default();
    let first = configured.first().and_then(|target| adapter(target));
    let enabled = configured.iter().any(|target| target == tool.target);

    let instruction_file = match first {
        Some(first) if !enabled => first.instruction_file,
        _ => own.instruction_file,
    };
    let reference_root = match first {
        Some(_) if enabled && root.join(own.reference_root).is_dir() => own.reference_root,
        Some(first) => first.reference_root,
        None => own.reference_root,
    };
    (reference_root, instruction_file)
}

/// The complete fluency prompt for a project, as `tool` finds its artifacts.
fn fluency(root: &Path, source_root: &Path, tool: &Tool, belay: Option<&BelayConfig>) -> String {
    let (reference_root, instruction_file) = layout(root, tool, belay);
    format!(
        "{}\n\n{}",
        primer(source_root, reference_root, instruction_file),
        reference(root).trim_end()
    )
}

/// The fluency prompt OpenCode reads for one run. OpenCode takes extra
/// instructions only as files, so the prompt is written to one for as long as
/// the tool runs.
struct PromptFile(PathBuf);

impl PromptFile {
    fn write(prompt: &str) -> std::io::Result<PromptFile> {
        let path = std::env::temp_dir().join(format!("piton-fluency-{}.md", std::process::id()));
        std::fs::write(&path, prompt)?;
        Ok(PromptFile(path))
    }
}

impl Drop for PromptFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The command that launches `tool` with `prompt` as part of its instructions,
/// with `args` passed through after it.
///
/// Each tool is given the prompt the way it adds to its own instructions
/// rather than replacing them: Claude appends it to the system prompt, Codex
/// and Muse take it as developer instructions, and OpenCode reads it as an
/// instructions file. OpenCode's configuration from the environment is read by
/// the server process, so it runs a private server rather than the shared
/// background one.
///
/// A tool's subcommand that rejects those options runs as given, without the
/// prompt; the one that takes them after its name gets them there.
fn command(tool: &Tool, prompt: &str, prompt_file: Option<&Path>, args: &[String]) -> Command {
    let mut command = Command::new(tool.program);
    let options: Vec<String> = match tool.name {
        "claude" => vec!["--append-system-prompt".to_string(), prompt.to_string()],
        "codex" => vec![
            "-c".to_string(),
            format!("developer_instructions={}", piton_belay::render::toml_string(prompt)),
        ],
        "opencode" => {
            let file = prompt_file.expect("OpenCode reads its prompt from a file");
            let config = serde_json::json!({ "instructions": [file.to_string_lossy()] });
            command.env("OPENCODE_CONFIG_CONTENT", config.to_string());
            vec!["--standalone".to_string()]
        }
        "muse" => vec![
            "-c".to_string(),
            format!("run.developer_prompt={}", serde_json::Value::String(prompt.to_string())),
        ],
        other => unreachable!("no launcher for `{other}`"),
    };

    let first = args.first().map(String::as_str);
    if first.is_some_and(|first| tool.plain_subcommands.contains(&first)) {
        command.args(args);
    } else if first.is_some() && first == tool.prompted_subcommand {
        command.arg(&args[0]).args(&options).args(&args[1..]);
    } else {
        command.args(&options).args(args);
    }
    command
}

pub fn run(agent: &str, print_fluency: bool, args: &[String]) -> u8 {
    let Some(tool) = TOOLS.iter().find(|tool| tool.name == agent) else {
        report::fail(format!("unknown agent `{agent}`"));
        return EXIT_ERRORS;
    };

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
    let prompt = fluency(&root, &project.source_root, tool, belay.as_ref());

    // Printing the prompt is read-only: nothing is built and nothing launched.
    if print_fluency {
        println!("{prompt}");
        return EXIT_SUCCESS;
    }

    let prompt_file = if tool.name == "opencode" {
        match PromptFile::write(&prompt) {
            Ok(file) => Some(file),
            Err(error) => {
                report::fail(format!("cannot write the fluency prompt for OpenCode: {error}"));
                return EXIT_ERRORS;
            }
        }
    } else {
        None
    };

    let program = tool.program;
    match command(tool, &prompt, prompt_file.as_ref().map(|file| file.0.as_path()), args)
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
                "`{program}` is not on PATH; install it or print the prompt with `piton agent {agent} --print-fluency`"
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

    fn launched(tool: &str, args: &[&str]) -> Vec<String> {
        let tool = TOOLS.iter().find(|candidate| candidate.name == tool).expect("tool");
        let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        command(tool, "Be \"fluent\".\nTwo lines.", Some(Path::new("/tmp/prompt.md")), &args)
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn each_tool_is_given_the_prompt_its_own_way() {
        assert_eq!(
            launched("claude", &["--resume"]),
            ["--append-system-prompt", "Be \"fluent\".\nTwo lines.", "--resume"]
        );
        let codex = launched("codex", &[]);
        assert_eq!(codex[0], "-c");
        assert!(codex[1].starts_with("developer_instructions=\"\"\"\n"), "{}", codex[1]);
        assert_eq!(
            launched("muse", &[]),
            ["-c", "run.developer_prompt=\"Be \\\"fluent\\\".\\nTwo lines.\""]
        );
        assert_eq!(launched("opencode", &["src"]), ["--standalone", "src"]);
    }

    #[test]
    fn subcommands_get_the_prompt_where_they_take_it() {
        assert_eq!(launched("muse", &["exec", "hi"])[..2], ["exec", "-c"]);
        assert_eq!(launched("muse", &["resume", "--last"]), ["resume", "--last"]);
        assert_eq!(launched("opencode", &["run", "hi"]), ["run", "--standalone", "hi"]);
        assert_eq!(launched("opencode", &["models"]), ["models"]);
    }

    #[test]
    fn the_primer_points_where_the_build_wrote() {
        let muse = TOOLS.iter().find(|tool| tool.name == "muse").expect("muse");
        let config = |adapters: &[&str]| BelayConfig {
            code_root: PathBuf::from("src"),
            shape_root: None,
            adapters: adapters.iter().map(|id| id.to_string()).collect(),
        };
        let nowhere = Path::new("/nonexistent-piton-project");
        // Without a configuration, the tool's own layout.
        assert_eq!(layout(nowhere, muse, None), (".muse/reference", "AGENTS.md"));
        // Shared with Claude Code, which owns the one reference tree.
        assert_eq!(
            layout(nowhere, muse, Some(&config(&["claude-code", "muse"]))),
            (".claude/reference", "AGENTS.md")
        );
        // Not built for Muse at all: what the project has.
        assert_eq!(
            layout(nowhere, muse, Some(&config(&["claude-code"]))),
            (".claude/reference", "CLAUDE.md")
        );
    }

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
