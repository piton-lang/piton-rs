//! `piton compile` — render one file, or a glob of files, through an adapter.

use std::path::{Path, PathBuf};

use piton_compile::{Compilation, Project, Symbol};
use piton_core::{Properties, Value};
use piton_emit::{Adapter, MarkdownContext};

use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(path: &str, adapter: &str, write: bool) -> u8 {
    let adapter: Adapter = match adapter.parse() {
        Ok(adapter) => adapter,
        Err(message) => {
            report::fail(message);
            return EXIT_ERRORS;
        }
    };

    let is_glob = path.contains('*') || path.contains('?') || path.contains('[');
    let inputs = if is_glob {
        match project::expand_glob(path) {
            Ok(found) => found,
            Err(message) => {
                report::fail(message);
                return EXIT_ERRORS;
            }
        }
    } else {
        vec![PathBuf::from(path)]
    };

    if inputs.is_empty() {
        report::fail(format!("`{path}` matched no .pi files"));
        return EXIT_ERRORS;
    }
    if is_glob && !write {
        // Several results cannot share one stdout stream in a way anyone could
        // use, so the glob form requires a destination.
        report::fail("a glob needs --write, because several results cannot share stdout");
        return EXIT_ERRORS;
    }

    let (configured, _) = project::current();
    let mut failed = false;

    for input in &inputs {
        if !input.is_file() {
            report::fail(format!("`{}` does not exist", input.display()));
            failed = true;
            continue;
        }
        let project = Project {
            entry: input.clone(),
            source_root: configured.source_root.clone(),
            root: configured.root.clone(),
            config_path: configured.config_path.clone(),
            frameworks: configured.frameworks.clone(),
        };
        let compilation = Compilation::build(project);
        if compilation.has_errors() {
            let root = compilation.project.root.clone();
            report::diagnostics(
                &compilation.diagnostics,
                &|p| compilation.source_of(p).map(str::to_string),
                &root,
            );
            failed = true;
            continue;
        }

        let rendered = render(&compilation, input, adapter);
        if write {
            let destination = input.with_extension(adapter.extension());
            if let Err(error) = std::fs::write(&destination, &rendered) {
                report::fail(format!("cannot write `{}`: {error}", destination.display()));
                failed = true;
                continue;
            }
            println!("{}", destination.display());
        } else {
            print!("{rendered}");
        }
    }

    if failed {
        EXIT_ERRORS
    } else {
        EXIT_SUCCESS
    }
}

/// Collects a file's own top-level declarations, in declaration order.
fn declarations(compilation: &Compilation, file: &Path) -> Properties {
    let mut out = Properties::new();
    let Some(module) = compilation.graph().id_for(file) else {
        return out;
    };
    for (name, symbol) in &compilation.resolution.scope(module).declarations {
        let value = match symbol {
            Symbol::Anchor(anchor) => {
                if compilation.store().anchor(*anchor).is_abstract {
                    // An abstract anchor declares a shape and has no value to
                    // emit on its own.
                    continue;
                }
                Value::Anchor(*anchor)
            }
            Symbol::Variable(variable) => compilation
                .store()
                .variable(*variable)
                .value
                .clone()
                .unwrap_or(Value::Null),
        };
        out.insert(name.clone(), value);
    }
    out
}

fn render(compilation: &Compilation, file: &Path, adapter: Adapter) -> String {
    let declarations = declarations(compilation, file);
    let directory = file.parent().unwrap_or(Path::new("."));
    piton_emit::render(
        adapter,
        &declarations,
        compilation,
        MarkdownContext {
            from_directory: directory,
            source_root: &compilation.project.source_root,
        },
    )
}
