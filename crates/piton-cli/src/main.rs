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

/// The exit codes of all commands. Success is 0. Reported errors are 1.
pub const EXIT_SUCCESS: u8 = 0;
pub const EXIT_ERRORS: u8 = 1;

/// The version that `piton --version` shows, for example `0.1.562`. The edge
/// workflow publishes this commit with this version. `build.rs` calculates it.
pub const VERSION: &str = env!("PITON_BUILD_VERSION");

#[derive(Parser)]
#[command(
    name = "piton",
    version = VERSION,
    about = "Compile Piton sources into agent guidance and structured data",
    long_about = "The Piton compiler turns declarative Piton sources into JSON, YAML, or \
Markdown, and compiles Belay projects into the artifacts agentic coding tools read."
)]
#[command(args_conflicts_with_subcommands = true, arg_required_else_help = true)]
struct Cli {
    /// Replace this piton with the latest release, and show the old and new
    /// versions. This is not `piton update`. That command updates packages
    #[arg(long)]
    update: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Build the project with the configuration in piton.config.pi
    Build {
        /// Path to a piton.config.pi file
        config: Option<PathBuf>,
        /// Report the files that the build would write. Do not write them
        #[arg(long)]
        dry_run: bool,
    },

    /// Check specific files or the project and report errors
    Check {
        /// Optional file, directory, or glob. The default is the project.
        /// When there is no piton.config.pi, the default is all .pi files
        /// below the working directory
        paths: Vec<PathBuf>,
    },

    /// Compile Piton
    ///
    /// For a single file, the compiled result goes to stdout. A glob path
    /// needs --write. Many results cannot use the same stream.
    Compile {
        /// File or glob
        path: String,
        /// Renderer: json, yaml, or markdown
        #[arg(long, default_value = "json")]
        renderer: String,
        /// Write each result next to its input, with the extension of the
        /// renderer
        #[arg(long)]
        write: bool,
        /// Put the result together with its source files. A build tool can
        /// then watch these files
        #[arg(long)]
        dependencies: bool,
    },

    /// Apply canonical formatting
    Format {
        /// File or glob. The default is the project. `-` reads the source from
        /// stdin and writes the formatted text to stdout
        path: Option<String>,
        /// Only check the files and report problems. Do not write
        #[arg(long)]
        check: bool,
    },

    /// Count lines of Piton source for specific files or the project
    Loc {
        /// Optional file, directory, or glob. The default is the project
        paths: Vec<PathBuf>,
    },

    /// Run the Piton language server
    Lsp {
        /// Use stdin and stdout. This is the only transport. Thus this flag
        /// changes nothing. Many language clients of editors send it. Thus
        /// the server accepts it.
        #[arg(long, hide = true)]
        stdio: bool,
    },

    /// Analyze which parts of the specbase are reachable
    ///
    /// Reports the reachable parts, the unreachable parts, the depth of each
    /// reachable anchor, and the path to it.
    Reach {
        /// Optional file, glob, or anchor at the start. The default is the
        /// entry of the project
        targets: Vec<String>,
        /// Do not list the unreachable parts
        #[arg(long = "no-unreachable")]
        no_unreachable: bool,
        /// Do not show the path to each reachable anchor
        #[arg(long = "no-paths")]
        no_paths: bool,
    },

    /// List the plugins of the project, their commands, and their permissions
    Plugins,

    /// Remove a package from the project
    Remove {
        /// Name of the package to remove
        package: String,
    },

    /// Print the part of the specification that one anchor or property needs
    ///
    /// Follows the anchors that the target extends, reads, refers to, and
    /// embeds. Also follows the names in its types. It follows them
    /// transitively. It prints the result as one Markdown document. This
    /// document is the context that an agent needs to implement the target.
    Slice {
        /// `file.pi#Anchor`, `file.pi#Anchor.property`, or a bare name to
        /// find in the project
        target: String,
    },

    /// Clone and "un-git" repositories into the tethers directory
    ///
    /// Without a source, installs all the dependencies in piton.config.pi.
    /// With a source, adds it to piton.config.pi and installs it.
    Tether {
        /// Optional URL (or path) of the git repository
        source: Option<String>,
        /// Install it with this name, not the name that it publishes
        #[arg(long = "as")]
        rename: Option<String>,
    },

    /// Move an installed package into the source root, and change its imports
    Untether {
        /// The name of the package to untether
        package: String,
        /// The name to give the untethered package
        #[arg(long = "as")]
        rename: Option<String>,
        /// Do not change imports. Only move the package
        #[arg(long = "no-rewrite")]
        no_rewrite: bool,
    },

    /// Update packages from the dependencies in piton.config.pi
    Update {
        /// Packages to update. When there are no names, update all packages
        packages: Vec<String>,
        /// Replace packages with edits after their installation. This discards
        /// the edits
        #[arg(long)]
        force: bool,
        /// Show a diff of each file that an update changes
        #[arg(long)]
        diff: bool,
    },
}

fn main() -> ExitCode {
    // Plugins add commands from their declarations. Thus, before it parses,
    // the CLI gets the built-in commands and the commands of all plugins of
    // the project.
    let (project, config_diagnostics, mut manifests, mut problems) = plugins::load();
    let mut command = Cli::command();
    plugins::withhold_builtins(&command, &mut manifests, &mut problems);
    command = plugins::add_commands(command, &manifests);
    let matches = command.get_matches();

    // Plugins that inspect projects examine the project before the command
    // runs. They offer to update a project for an older Piton. They do not
    // run before the language server. Its stdout is the protocol of the editor.
    if matches.subcommand_name() != Some("lsp") && !matches.get_flag("update") {
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
    let Some(command) = cli.command else {
        // Because of `arg_required_else_help`, only `--update` comes here.
        let code = commands::self_update::run();
        report::flush();
        return ExitCode::from(code);
    };
    let code = match command {
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
    // Problems always print last, after all other output of the command.
    report::flush();
    ExitCode::from(code)
}
