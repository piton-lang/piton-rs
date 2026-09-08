//! The `piton` command-line compiler.

mod commands;
mod files;
mod report;
mod session;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use commands::Format;

#[derive(Parser)]
#[command(
    name = "piton",
    version,
    about = "The Piton compiler",
    long_about = "Compile Piton (.pi) source into data, or into agentic Markdown through a \
                  framework such as Belay."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile Piton files into data.
    Compile {
        /// Files, directories, or globs.
        #[arg(required = true)]
        paths: Vec<String>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
        /// Write output here instead of beside each input.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Print to stdout instead of writing files.
        #[arg(long)]
        stdout: bool,
    },
    /// Check Piton files for errors without writing anything.
    Check {
        /// Files, directories, or globs.
        #[arg(required = true)]
        paths: Vec<String>,
    },
    /// Build the project described by piton.config.pi.
    Build {
        #[command(subcommand)]
        command: Option<BuildCommand>,
    },
    /// List which files under the project root are compiled, and which are not.
    Reach {
        /// Show only one half of the answer.
        #[arg(long, value_enum, default_value_t = commands::Reach::All)]
        show: commands::Reach,
        /// Exit non-zero when any file is unreached.
        #[arg(long)]
        strict: bool,
        /// Print one explicit `a -> b -> c` per file instead of a tree.
        #[arg(long)]
        chains: bool,
        /// Measure from this file instead of the project's entry point.
        #[arg(long, value_name = "FILE")]
        entry: Option<PathBuf>,
    },
    /// Apply the canonical formatting.
    Format {
        /// Files, directories, globs, or `-` for stdin.
        #[arg(required = true)]
        paths: Vec<String>,
        /// Report unformatted files instead of rewriting them.
        #[arg(long)]
        check: bool,
    },
    /// Run the language server over stdio.
    Lsp {
        /// Accepted and ignored: the server only ever speaks over stdio.
        ///
        /// Clients built on `vscode-languageclient` append this to the command
        /// they were configured with, so the server has to tolerate it.
        #[arg(long)]
        stdio: bool,
    },
    /// Generate editor grammars and extensions.
    Grammar {
        /// Where to write them.
        #[arg(default_value = "editors")]
        out_dir: PathBuf,
        /// Git remote the Tree-sitter grammar is published to.
        #[arg(long, value_name = "URL")]
        grammar_repo: Option<String>,
        /// Commit of the published grammar. Editors need a full SHA.
        #[arg(long, value_name = "SHA")]
        grammar_rev: Option<String>,
    },
    /// Print the generated language reference.
    Docs {
        /// Write to a file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Start the Claude CLI with Piton and framework fluency preloaded.
    Claude {
        /// Print the fluency brief instead of launching.
        #[arg(long)]
        print_prompt: bool,
        /// Install the brief as a reusable Claude Code skill.
        #[arg(long)]
        install: bool,
        /// Arguments passed straight through to `claude`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Print a file's concrete syntax tree.
    Ast {
        path: PathBuf,
    },
}

#[derive(Subcommand)]
enum BuildCommand {
    /// Build and report problems without writing output.
    Check,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Compile { paths, format, out_dir, stdout } => {
            commands::compile(&paths, format, out_dir.as_deref(), stdout, true)?
        }
        Command::Check { paths } => commands::compile(&paths, Format::Json, None, false, false)?,
        Command::Build { command } => commands::build(command.is_some())?,
        Command::Reach { show, strict, chains, entry } => {
            commands::reach(show, strict, chains, entry)?
        }
        Command::Format { paths, check } => commands::format(&paths, check)?,
        Command::Lsp { stdio: _ } => {
            piton_lsp::run(session::registry)?;
            0
        }
        Command::Grammar { out_dir, grammar_repo, grammar_rev } => {
            commands::grammar(&out_dir, grammar_repo, grammar_rev)?
        }
        Command::Docs { out } => commands::docs(out.as_deref())?,
        Command::Claude { print_prompt, install, args } => {
            commands::claude(&args, print_prompt, install)?
        }
        Command::Ast { path } => commands::ast(&path)?,
    };
    std::process::exit(code);
}
