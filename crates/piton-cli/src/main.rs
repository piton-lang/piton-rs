//! The Piton command-line compiler.

mod commands;
mod config_edit;
mod packages;
mod project;
mod render;
mod report;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Exit codes the commands agree on: success is 0, reported errors are 1.
pub const EXIT_SUCCESS: u8 = 0;
pub const EXIT_ERRORS: u8 = 1;

/// The version `piton --version` reports.
///
/// A release build sets `PITON_VERSION` when it compiles, to the version it is
/// published as, like `0.1.562`. A local build falls back to the crate
/// version.
const VERSION: &str = match option_env!("PITON_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Parser)]
#[command(
    name = "piton",
    version = VERSION,
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
        /// Print the entire fluency prompt instead of launching the agent
        #[arg(long)]
        print_fluency: bool,
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
        /// Optional file, directory, or glob. Defaults to the project, or to
        /// every .pi file under the working directory when there is no
        /// piton.config.pi
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
        /// Renderer: json, yaml, or markdown
        #[arg(long, default_value = "json")]
        renderer: String,
        /// Write each result next to its input with the renderer's extension
        #[arg(long)]
        write: bool,
        /// Wrap the result with the source files it was compiled from, for a
        /// build tool that has to watch them
        #[arg(long)]
        dependencies: bool,
    },

    /// Apply canonical formatting
    Format {
        /// File or glob; defaults to the project. `-` reads the source from
        /// stdin and writes the formatted text to stdout
        path: Option<String>,
        /// Only check the files and report problems; do not write
        #[arg(long)]
        check: bool,
    },

    /// Count lines of Piton source for specific files or the project
    Loc {
        /// Optional file, directory, or glob; defaults to the project
        paths: Vec<PathBuf>,
    },

    /// Run the Piton language server
    Lsp {
        /// Talk over stdin and stdout. That is the only transport, so this
        /// changes nothing; it is accepted because many editors' language
        /// clients pass it.
        #[arg(long, hide = true)]
        stdio: bool,
    },

    /// Analyze which parts of the specbase are reachable
    ///
    /// Reports what is reachable, what is unreachable, the depth of each
    /// reachable anchor, and the path taken to it.
    Reach {
        /// Optional file, glob, or anchor to start from; defaults to the
        /// project's entry
        targets: Vec<String>,
        /// Don't list what nothing reaches
        #[arg(long = "no-unreachable")]
        no_unreachable: bool,
        /// Don't show the path taken to each reachable anchor
        #[arg(long = "no-paths")]
        no_paths: bool,
    },

    /// Remove a package from the project
    Remove {
        /// Name of the package to remove
        package: String,
    },

    /// Clone and "un-git" repositories into the tethers directory
    ///
    /// On its own, installs everything in piton.config.pi's dependencies.
    /// Given a source, adds it to piton.config.pi and installs it.
    Tether {
        /// Optional URL (or path) of the git repository
        source: Option<String>,
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
        Command::Agent {
            agent,
            print_fluency,
            args,
        } => commands::agent::run(&agent, print_fluency, &args),
        Command::Build { config, dry_run } => commands::build::run(config.as_deref(), dry_run),
        Command::Check { paths } => commands::check::run(&paths),
        Command::Compile {
            path,
            renderer,
            write,
            dependencies,
        } => commands::compile::run(&path, &renderer, write, dependencies),
        Command::Format { path, check } => commands::format::run(path.as_deref(), check),
        Command::Loc { paths } => commands::loc::run(&paths),
        Command::Lsp { .. } => commands::lsp::run(),
        Command::Reach {
            targets,
            no_unreachable,
            no_paths,
        } => commands::reach::run(&targets, !no_unreachable, !no_paths),
        Command::Remove { package } => commands::remove::run(&package),
        Command::Tether { source, rename } => {
            commands::tether::run(source.as_deref(), rename.as_deref())
        }
        Command::Untether {
            package,
            rename,
            no_rewrite,
        } => commands::untether::run(&package, rename.as_deref(), no_rewrite),
        Command::Update { packages } => commands::update::run(&packages),
    };
    ExitCode::from(code)
}
