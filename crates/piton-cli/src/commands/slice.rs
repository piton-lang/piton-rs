//! `piton slice` — print the part of the specification that one item needs.
//!
//! The target is `file.pi#Anchor`, `file.pi#Anchor.property`, or a bare name.
//! The command finds a bare name in the project. The result is one Markdown
//! document on stdout. The command gives this document to an agent as it is.
//!
//! A plugin can make the slice cite its compiled output, not the source. An
//! example is `piton belay slice --adapter` of Belay. The plugin gives
//! [`cite`] the compiled location of each anchor. This module finds and prints
//! the slice in the same way. Then the slice never cites the source. Each
//! part that the slice reaches has a location in the compiled output. That location can
//! be only the documents of the anchors that extend or read it.

use piton_compile::slice::{self, Target};
use piton_compile::Compilation;
use std::path::{Path, PathBuf};

use piton_core::{diagnostics, Diagnostic, Ref, Span};
use piton_emit::markdown::LinkResolver;

use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(target: &str) -> u8 {
    let target = match Target::parse(target) {
        Ok(target) => target,
        Err(message) => {
            report::fail(message);
            return EXIT_ERRORS;
        }
    };

    let (configured, _) = project::enclosing();
    let (compilation, entry) = match &target.file {
        // The command builds the project. Thus it can trace the chain from
        // its entry. When that build does not reach a file, the file compiles
        // from itself. That file has no chain.
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
        // Any file can declare a bare name, also a file that no file imports
        // at this time. Thus the command compiles the full source tree to find it.
        None => {
            let mut project = configured;
            if !project.entry.is_file() {
                // Without a configuration, there is no entry. The compilation
                // of the workspace needs only one entry to start.
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

    let resolved = match &entry {
        Some(path) => match compilation.graph().id_for(path) {
            Some(module) => slice::resolve_in(&compilation, module, &target),
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
            return EXIT_ERRORS;
        }
    };

    let slice = slice::slice(&compilation, entity);
    print!("{}", slice::markdown::render(&compilation, &slice, &here()));
    EXIT_SUCCESS
}

/// The directory where the command runs. All printed paths are relative to
/// this directory.
fn here() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// The locations of a target, relative to the project root. This function
/// makes them relative to the directory where the command runs. Thus the
/// reader can follow them from that directory. An example is
/// `../../.claude/reference/ui.md#color` from `spec/components`.
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

/// The module file that `file` names. A directory names its index. Returns
/// `None` after it reports a problem.
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

/// Prints a target that did not resolve. When the problem has a source line,
/// it prints that line too.
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

/// Prints the slice of `target` from a compilation of the caller. The caller
/// compiled from the entry of the project, as a build does. For each anchor,
/// the slice cites the compiled location that `links` gives. Returns the exit
/// code.
pub fn cite(compilation: &Compilation, target: &str, links: &dyn LinkResolver) -> u8 {
    let target = match Target::parse(target) {
        Ok(target) => target,
        Err(message) => {
            report::fail(message);
            return EXIT_ERRORS;
        }
    };
    let root = compilation.project.root.clone();
    let entry = match &target.file {
        Some(file) => {
            let Some(path) = existing(file) else {
                return EXIT_ERRORS;
            };
            Some(path)
        }
        None => None,
    };
    let resolved = match &entry {
        Some(path) => match compilation.graph().id_for(path) {
            Some(module) => slice::resolve_in(compilation, module, &target),
            None => {
                report::fail(format!(
                    "`{}` is not part of what the build compiles, so nothing in it has a compiled location",
                    project::display(path, &root)
                ));
                report::help("import it from the entry, or slice the source with `piton slice`");
                return EXIT_ERRORS;
            }
        },
        None => slice::find(compilation, &target),
    };
    let entity = match resolved {
        Ok(entity) => entity,
        Err(diagnostic) => {
            fail(compilation, &diagnostic);
            if entry.is_none() {
                report::help("only what the build compiles is searched; `piton slice` searches every file");
            }
            return EXIT_ERRORS;
        }
    };

    // The dependencies of the target compile into the anchors that use them.
    // But the target must have a location. If not, the slice cannot refer to
    // it.
    if let slice::Entity::Anchor(anchor) | slice::Entity::Property(anchor, _) = &entity {
        let def = compilation.store().anchor(*anchor);
        let target = piton_core::Ref::anchor(*anchor);
        if !def.is_abstract && slice::markdown::compiled_location(compilation, links, &target).is_none() {
            report::fail(format!(
                "`{}` is never compiled: nothing the build emits exports, references, or embeds it",
                def.name
            ));
            report::help("export it from the entry, or slice the source with `piton slice`");
            return EXIT_ERRORS;
        }
    }

    let slice = slice::slice(compilation, entity);
    print!(
        "{}",
        slice::markdown::render_compiled(
            compilation,
            &slice,
            &Rebased {
                links,
                root: &root,
                here: here(),
            }
        )
    );
    EXIT_SUCCESS
}
