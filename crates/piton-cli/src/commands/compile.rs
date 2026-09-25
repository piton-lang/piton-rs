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
        // Compiled from its absolute path, so every module the graph records
        // -- and every path `--dependencies` reports -- is absolute too.
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

/// Renders what a file compiles to: an object with one key per export.
fn render(compilation: &Compilation, file: &Path, renderer: Adapter) -> String {
    let Some(module) = compilation.graph().id_for(file) else {
        return String::new();
    };
    // The result is written next to the input, or read as if it were.
    let directory = file.parent().unwrap_or(Path::new("."));
    render::render_module(compilation, module, renderer, directory, None)
}

/// Wraps a rendered result with the source files it was compiled from.
///
/// A build tool that imports a `.pi` file has to know which other files to
/// watch, and the only thing that knows is the compilation: imports resolve
/// through the module graph, and a package may put a file somewhere the
/// importing text never names. So the compiler says, rather than the build
/// tool guessing.
///
/// Bundled package files are left out. They live inside the binary rather than
/// on disk, so nothing can watch them and their contents only change when the
/// compiler itself does.
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

    // The rendered output is already text in the adapter's own format, so it
    // is carried as a string rather than spliced in as JSON: a Markdown render
    // is not JSON, and a caller that wants the value parses it themselves.
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
