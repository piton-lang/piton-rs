//! `piton compile` — render one file, or a glob of files, through a renderer.

use std::path::{Path, PathBuf};

use piton_compile::Compilation;
use piton_emit::Adapter;

use anstream::println;

use crate::{project, render, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(path: &str, renderer: &str, write: bool, dependencies: bool) -> u8 {
    let renderer: Adapter = match render::parse_renderer(renderer) {
        Ok(renderer) => renderer,
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
        // Many results in one stdout stream are not usable. Thus the glob form
        // needs a destination.
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
        // The file compiles from its absolute path. Thus all modules in the
        // graph have absolute paths. All paths from `--dependencies` are also
        // absolute.
        let absolute = project::canonical_target(input);
        let project = configured.with_entry(&absolute);
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

        let mut rendered = render(&compilation, &absolute, renderer);
        if dependencies {
            rendered = envelope(&compilation, &rendered);
        }
        if write {
            let destination = input.with_extension(renderer.extension());
            if let Err(error) = std::fs::write(&destination, &rendered) {
                report::fail(format!("cannot write `{}`: {error}", destination.display()));
                failed = true;
                continue;
            }
            println!("{}", report::added(destination.display()));
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

/// Renders the compiled output of a file. This is an object with one key for
/// each export.
fn render(compilation: &Compilation, file: &Path, renderer: Adapter) -> String {
    let Some(module) = compilation.graph().id_for(file) else {
        return String::new();
    };
    // The result goes next to the input, or the reader uses it as if it is
    // there.
    let directory = file.parent().unwrap_or(Path::new("."));
    render::render_module(compilation, module, renderer, directory, None)
}

/// Puts a rendered result together with its source files.
///
/// A build tool that imports a `.pi` file must know the other files to watch.
/// Only the compilation knows them. Imports resolve through the module graph.
/// A package can put a file in a location that the import text does not name.
/// Thus the compiler gives the list. The build tool does not guess.
///
/// The list does not include bundled package files. They are in the binary,
/// not on disk. Thus no tool can watch them. Their contents change only when
/// the compiler changes.
fn envelope(compilation: &Compilation, rendered: &str) -> String {
    let mut paths: Vec<String> = compilation
        .graph()
        .iter()
        .map(|module| module.path.clone())
        .filter(|path| !path.to_string_lossy().starts_with('@'))
        .map(|path| path.to_string_lossy().to_string())
        .collect();
    paths.sort();
    paths.dedup();

    let list = paths
        .iter()
        .map(|path| format!("    {}", json_string(path)))
        .collect::<Vec<_>>()
        .join(",\n");

    // The rendered output is text in the format of the adapter. Thus
    // it stays a string, and it does not go into the JSON as JSON. A Markdown
    // render is not JSON. A caller that needs the value parses it.
    format!(
        "{{\n  \"value\": {},\n  \"dependencies\": [\n{list}\n  ]\n}}\n",
        json_string(rendered.trim_end())
    )
}

/// Quotes a string as JSON.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
