//! The Piton command-line compiler.

mod commands;
mod config_edit;
mod packages;
mod project;
mod render;
mod report;
mod style;
mod diff;
mod inspection;
mod plugins;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

/// Exit codes the commands agree on: success is 0, reported errors are 1.
pub const EXIT_SUCCESS: u8 = 0;
pub const EXIT_ERRORS: u8 = 1;

/// The version `piton --version` reports: the one the edge workflow
/// publishes this commit as, like `0.1.562`. `build.rs` works it out.
const VERSION: &str = env!("PITON_BUILD_VERSION");

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

    /// Start a new project from one of the built-in templates
    ///
    /// Without a directory, asks for the project's name and creates it in a
    /// directory of that name. Without --template, asks which one to use, and
    /// for a Belay template which adapters to build for. Nothing is overwritten: if any file the
    /// template writes already exists, nothing is written. A template that
    /// declares dependencies, like crux/project, has them tethered.
    Init {
        /// Directory to create the project in; asked for as the project's
        /// name, or the current one when there is no terminal to ask on
        directory: Option<PathBuf>,
        /// Template to start from; `--list` names them
        #[arg(long, short)]
        template: Option<String>,
        /// Belay adapter to build for, like `codex`; repeat it or separate
        /// with commas for several
        #[arg(long, short, value_delimiter = ',')]
        adapter: Vec<String>,
        /// List the templates and what each one sets up
        #[arg(long, conflicts_with_all = ["directory", "template", "adapter", "no_tether"])]
        list: bool,
        /// Write the files only, leaving a template's dependencies for
        /// `piton tether`
        #[arg(long)]
        no_tether: bool,
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

    /// List the project's plugins, the commands they add, and what each may do
    Plugins,

    /// Remove a package from the project
    Remove {
        /// Name of the package to remove
        package: String,
    },

    /// Print the part of the specification one anchor or property depends on
    ///
    /// Follows what the target extends, reads, refers to, and embeds, and
    /// what its types name, transitively, and prints it as one Markdown
    /// document: the context an agent needs to implement that one thing.
    Slice {
        /// `file.pi#Anchor`, `file.pi#Anchor.property`, or a bare name to
        /// look up across the project
        target: String,
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
        /// Replace packages that were edited since they were installed,
        /// discarding the edits
        #[arg(long)]
        force: bool,
        /// Show a diff of every file each update changes
        #[arg(long)]
        diff: bool,
    },
}

fn main() -> ExitCode {
    // Plugins add commands from their declarations, so the CLI is built from
    // the built-in commands and every plugin the project has, before parsing.
    let (project, config_diagnostics, mut manifests, mut problems) = plugins::load();
    let mut command = Cli::command();
    plugins::withhold_builtins(&command, &mut manifests, &mut problems);
    command = plugins::add_commands(command, &manifests);
    let matches = command.get_matches();

    // Plugins that inspect projects look before the command runs, and offer
    // to update one written for an older Piton. Not before the language
    // server, whose stdout is the editor's protocol.
    if matches.subcommand_name() != Some("lsp") {
        let running = plugins::matched(&manifests, &matches).map(|(manifest, _, _)| manifest.name.clone());
        if let Some(code) = inspection::offer(&project, &manifests, running.as_deref()) {
            report::flush();
            return ExitCode::from(code);
        }
    }

    if let Some((manifest, spec, arguments)) = plugins::matched(&manifests, &matches) {
        let code = plugins::run(project, config_diagnostics, &manifests, &problems, manifest, spec, arguments);
        report::flush();
        return ExitCode::from(code);
    }
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    let code = match cli.command {
        Command::Build { config, dry_run } => commands::build::run(config.as_deref(), dry_run),
        Command::Check { paths } => commands::check::run(&paths),
        Command::Compile {
            path,
            renderer,
            write,
            dependencies,
        } => commands::compile::run(&path, &renderer, write, dependencies),
        Command::Format { path, check } => commands::format::run(path.as_deref(), check),
        Command::Init {
            directory,
            template,
            adapter,
            list,
            no_tether,
        } => commands::init::run(
            directory.as_deref(),
            template.as_deref(),
            &adapter,
            list,
            no_tether,
        ),
        Command::Loc { paths } => commands::loc::run(&paths),
        Command::Lsp { .. } => commands::lsp::run(),
        Command::Reach {
            targets,
            no_unreachable,
            no_paths,
        } => commands::reach::run(&targets, !no_unreachable, !no_paths),
        Command::Plugins => commands::plugins::run(),
        Command::Remove { package } => commands::remove::run(&package),
        Command::Slice { target } => commands::slice::run(&target),
        Command::Tether { source, rename } => {
            commands::tether::run(source.as_deref(), rename.as_deref())
        }
        Command::Untether {
            package,
            rename,
            no_rewrite,
        } => commands::untether::run(&package, rename.as_deref(), no_rewrite),
        Command::Update {
            packages,
            force,
            diff,
        } => commands::update::run(&packages, force, diff),
    };
    // Problems are printed last, whatever the command printed before them.
    report::flush();
    ExitCode::from(code)
}
