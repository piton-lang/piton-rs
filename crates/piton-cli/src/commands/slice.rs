//! `piton slice` — print the part of the specification one thing depends on.
//!
//! The target is `file.pi#Anchor`, `file.pi#Anchor.property`, or a bare name
//! looked up across the project. The result is one Markdown document on
//! stdout, meant to be handed to an agent as-is.
//!
//! With `--adapter`, the document cites what that Belay target compiles
//! rather than the source, for an agent that reads the compiled reference
//! documents. The locations have to be the ones `piton build` writes, so the
//! project is compiled from its own entry, exactly as a build compiles it,
//! rather than from the target's file. The source is never cited: whatever
//! the slice reaches from there was compiled somewhere, if only into the
//! documents of what extends or reads it.

use piton_compile::slice::{self, Target};
use piton_compile::Compilation;
use std::path::{Path, PathBuf};

use piton_core::{diagnostics, Diagnostic, Ref, Span};
use piton_emit::markdown::LinkResolver;

use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(target: &str, adapter: Option<&str>) -> u8 {
    let target = match Target::parse(target) {
        Ok(target) => target,
        Err(message) => {
            report::fail(message);
            return EXIT_ERRORS;
        }
    };

    let (configured, _) = project::enclosing();
    if let Some(adapter) = adapter {
        if configured.belay().is_none() {
            report::fail(format!(
                "`--adapter {adapter}` cites what a Belay target compiles, and this project has no Belay configuration"
            ));
            return EXIT_ERRORS;
        }
    }
    let (compilation, entry) = match &target.file {
        // With an adapter, only what the build compiles has a location, so
        // the target has to be found in that same compilation.
        Some(file) if adapter.is_some() => {
            let Some(path) = existing(file) else {
                return EXIT_ERRORS;
            };
            (Compilation::build(configured), Some(path))
        }
        None if adapter.is_some() => (Compilation::build(configured), None),
        // The project's own build, so the chain from its entry can be traced;
        // a file that build doesn't reach is compiled from itself instead,
        // and has no chain.
        Some(file) => {
            let Some(path) = existing(file) else {
                return EXIT_ERRORS;
            };
            let build = configured
                .entry
                .is_file()
                .then(|| Compilation::build(configured.clone()))
                .filter(|build| build.graph().id_for(&path).is_some());
            match build {
                Some(build) => (build, Some(path)),
                None => (Compilation::build(configured.with_entry(&path)), Some(path)),
            }
        }
        // A bare name could be declared anywhere, including a file nothing
        // imports yet, so the whole source tree is compiled to look for it.
        None => {
            let mut project = configured;
            if !project.entry.is_file() {
                // Without a configuration there is no entry, and compiling
                // the workspace only needs one to start from.
                match piton_compile::module::sources(&project.source_root).first() {
                    Some(first) => project = project.with_entry(first),
                    None => {
                        report::fail(format!(
                            "no .pi files to look for `{}` in",
                            target.display()
                        ));
                        return EXIT_ERRORS;
                    }
                }
            }
            (
                Compilation::build_workspace(project, Default::default(), &[]),
                None,
            )
        }
    };
    let root = compilation.project.root.clone();

    if compilation.has_errors() {
        report::diagnostics(
            &compilation.diagnostics,
            &|p| compilation.source_of(p).map(str::to_string),
            &root,
        );
        return EXIT_ERRORS;
    }

    // An adapter the project does not build is reported before the target,
    // since no target could be cited there.
    let locations = match adapter {
        Some(adapter) => {
            let config = compilation
                .project
                .belay()
                .expect("the Belay configuration was checked before compiling");
            match piton_belay::locations(&compilation, config, adapter) {
                Ok(locations) => Some(locations),
                Err(message) => {
                    report::fail(message);
                    return EXIT_ERRORS;
                }
            }
        }
        None => None,
    };

    let resolved = match &entry {
        Some(path) => match compilation.graph().id_for(path) {
            Some(module) => slice::resolve_in(&compilation, module, &target),
            None if adapter.is_some() => {
                report::fail(format!(
                    "`{}` is not part of what the build compiles, so nothing in it has a compiled location",
                    project::display(path, &root)
                ));
                report::help("import it from the entry, or slice without --adapter");
                return EXIT_ERRORS;
            }
            None => {
                report::fail(format!("cannot read `{}`", path.display()));
                return EXIT_ERRORS;
            }
        },
        None => slice::find(&compilation, &target),
    };
    let entity = match resolved {
        Ok(entity) => entity,
        Err(diagnostic) => {
            fail(&compilation, &diagnostic);
            if adapter.is_some() && entry.is_none() {
                report::help("with --adapter, only what the build compiles is searched; without it, every file is");
            }
            return EXIT_ERRORS;
        }
    };

    // What the target depends on is compiled into whatever uses it, but the
    // target itself has to be somewhere, or there is nothing to point at.
    if let (
        Some(locations),
        slice::Entity::Anchor(anchor) | slice::Entity::Property(anchor, _),
    ) = (&locations, &entity)
    {
        let def = compilation.store().anchor(*anchor);
        let target = piton_core::Ref::anchor(*anchor);
        if !def.is_abstract
            && slice::markdown::compiled_location(&compilation, locations, &target).is_none()
        {
            report::fail(format!(
                "`{}` is never compiled: nothing the build emits exports, references, or embeds it",
                def.name
            ));
            report::help("export it from the entry, or slice without --adapter to cite the source");
            return EXIT_ERRORS;
        }
    }

    let slice = slice::slice(&compilation, entity);
    let Some(locations) = locations else {
        print!("{}", slice::markdown::render(&compilation, &slice, &here()));
        return EXIT_SUCCESS;
    };

    print!(
        "{}",
        slice::markdown::render_compiled(
            &compilation,
            &slice,
            &Rebased {
                links: &locations,
                root: &root,
                here: here(),
            }
        )
    );
    EXIT_SUCCESS
}

/// The directory the command runs in, which every path printed is written
/// relative to.
fn here() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// A target's locations, which are relative to the project root, rewritten
/// relative to where the command runs, so the reader can follow them from
/// there: `../../.claude/reference/ui.md#color` from `spec/components`.
struct Rebased<'a> {
    links: &'a dyn LinkResolver,
    root: &'a Path,
    here: PathBuf,
}

impl LinkResolver for Rebased<'_> {
    fn link(&self, target: &Ref) -> Option<String> {
        let link = self.links.link(target)?;
        let (file, fragment) = match link.split_once('#') {
            Some((file, fragment)) => (file, Some(fragment)),
            None => (link.as_str(), None),
        };
        let file = piton_emit::relative_link(&self.here, &self.root.join(file));
        Some(match fragment {
            Some(fragment) => format!("{file}#{fragment}"),
            None => file,
        })
    }
}

/// The module file `file` names, a directory standing for its index, or
/// `None` once that has been reported.
fn existing(file: &Path) -> Option<PathBuf> {
    let mut path = project::canonical_target(file);
    if path.is_dir() {
        path = path.join("index.pi");
    }
    if path.is_file() {
        Some(path)
    } else {
        report::fail(format!("`{}` does not exist", file.display()));
        None
    }
}

/// Prints a target that did not resolve, with the source line when the
/// problem has one.
fn fail(compilation: &Compilation, diagnostic: &Diagnostic) {
    let source = if diagnostic.span == Span::default() {
        None
    } else {
        compilation.source_of(&diagnostic.file)
    };
    eprint!(
        "{}",
        diagnostics::render(diagnostic, source, Some(&compilation.project.root))
    );
}
