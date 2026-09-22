//! The Piton command-line compiler.

mod commands;
mod project;
mod report;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Exit codes the commands agree on: success is 0, reported errors are 1.
pub const EXIT_SUCCESS: u8 = 0;
pub const EXIT_ERRORS: u8 = 1;

#[derive(Parser)]
#[command(
    name = "piton",
    version,
    about = "Compile Piton sources into agent guidance and structured data",
    long_about = "The Piton compiler turns declarative Piton sources into JSON, YAML, or \
Markdown, and compiles Belay projects into the artifacts agentic coding tools read."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Launch the specified agent with Piton fluency
    Agent {
        /// Which agent to run
        #[arg(value_parser = ["claude"], default_value = "claude")]
        agent: String,
        /// Arguments passed through to the agent
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run semantic analysis on the project
    ///
    /// Combines linguistic analysis of the prose with structural knowledge of
    /// the Piton source to report statements that contradict each other.
    Analyze {
        /// Files or anchor names to analyze; defaults to the whole project
        targets: Vec<String>,
        /// Show every piece of evidence behind a finding
        #[arg(long)]
        explain: bool,
        /// List every claim that was extracted, and stop
        #[arg(long)]
        claims: bool,
        /// Report how much of the specbase the analysis could read, and stop
        #[arg(long)]
        coverage: bool,
        /// Output format: human, or interpretation for a restatement of what
        /// the analysis understood
        #[arg(long)]
        format: Option<String>,
        /// Lowest severity to report: error, warning, or information
        #[arg(long)]
        min_severity: Option<String>,
    },

    /// Build the project as configured by piton.config.pi
    Build {
        /// Path to a piton.config.pi file
        config: Option<PathBuf>,
        /// Report what would be written without writing it
        #[arg(long)]
        dry_run: bool,
    },

    /// Check specific files or the project and report errors
    Check {
        /// Files or directories to check; defaults to the project
        paths: Vec<PathBuf>,
    },

    /// Compile Piton
    ///
    /// Pointing at a single file writes the compiled result to stdout.
    /// Glob-based paths need --write, because several results cannot share
    /// one stream.
    Compile {
        /// File or glob
        path: String,
        /// Output format
        #[arg(long, default_value = "json")]
        adapter: String,
        /// Write each result next to its input with the adapter's extension
        #[arg(long)]
        write: bool,
        /// Wrap the result with the source files it was compiled from, for a
        /// build tool that has to watch them
        #[arg(long)]
        dependencies: bool,
    },

    /// Apply canonical formatting
    Format {
        /// File or glob; defaults to the project
        path: Option<String>,
        /// Only check the files and report problems; do not write
        #[arg(long)]
        check: bool,
    },

    /// Count lines of Piton source for specific files or the project
    Loc {
        /// Files or directories to count; defaults to the project
        paths: Vec<PathBuf>,
    },

    /// Run the Piton language server
    Lsp,

    /// Analyze which parts of the specbase are reachable
    Reach {
        /// Files or anchor names to start from; defaults to the project entry
        targets: Vec<String>,
        /// Also list what nothing reaches
        #[arg(long, default_value_t = true)]
        unreachable: bool,
        /// Show the path taken to each reachable anchor
        #[arg(long)]
        paths: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Agent { agent, args } => commands::agent::run(&agent, &args),
        Command::Analyze {
            targets,
            explain,
            claims,
            coverage,
            format,
            min_severity,
        } => commands::analyze::run(
            &targets,
            explain,
            claims,
            coverage,
            format.as_deref(),
            min_severity.as_deref(),
        ),
        Command::Build { config, dry_run } => commands::build::run(config.as_deref(), dry_run),
        Command::Check { paths } => commands::check::run(&paths),
        Command::Compile {
            path,
            adapter,
            write,
            dependencies,
        } => commands::compile::run(&path, &adapter, write, dependencies),
        Command::Format { path, check } => commands::format::run(path.as_deref(), check),
        Command::Loc { paths } => commands::loc::run(&paths),
        Command::Lsp => commands::lsp::run(),
        Command::Reach {
            targets,
            unreachable,
            paths,
        } => commands::reach::run(&targets, unreachable, paths),
    };
    ExitCode::from(code)
}
