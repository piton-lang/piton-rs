//! The Piton command-line compiler.

mod commands;
mod packages;
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

    /// Remove a package from the project
    Remove {
        /// Name of the package to remove
        package: String,
    },

    /// Clone and "un-git" a repository into the tethers directory
    Tether {
        /// Path to the git repository
        source: String,
        /// Install it under this name instead of the one it publishes
        #[arg(long = "as")]
        rename: Option<String>,
    },

    /// Move an installed package into the source root, rewriting its imports
    Untether {
        /// The name of the package to untether
        package: String,
        /// The name to give the untethered package
        #[arg(long = "as")]
        rename: Option<String>,
        /// Don't rewrite any imports; simply move the package
        #[arg(long = "no-rewrite")]
        no_rewrite: bool,
    },

    /// Update packages from the dependencies in piton.config.pi
    Update {
        /// Packages to update; all of them when none are named
        packages: Vec<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Agent { agent, args } => commands::agent::run(&agent, &args),
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
        Command::Remove { package } => commands::remove::run(&package),
        Command::Tether { source, rename } => commands::tether::run(&source, rename.as_deref()),
        Command::Untether {
            package,
            rename,
            no_rewrite,
        } => commands::untether::run(&package, rename.as_deref(), no_rewrite),
        Command::Update { packages } => commands::update::run(&packages),
    };
    ExitCode::from(code)
}
