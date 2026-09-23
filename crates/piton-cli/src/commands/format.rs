//! `piton format` — apply canonical formatting.

use std::path::PathBuf;

use piton_syntax::format;

use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(path: Option<&str>, check_only: bool) -> u8 {
    if path == Some("-") {
        return format_stdin(check_only);
    }
    let (configured, _) = project::current();
    let fallback = if configured.config_path.is_some() {
        configured.source_root.clone()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    let sources = match path {
        Some(pattern) if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') => {
            match project::expand_glob(pattern) {
                Ok(found) => found,
                Err(message) => {
                    report::fail(message);
                    return EXIT_ERRORS;
                }
            }
        }
        Some(pattern) => match project::collect_sources(&[PathBuf::from(pattern)], &fallback) {
            Ok(found) => found,
            Err(message) => {
                report::fail(message);
                return EXIT_ERRORS;
            }
        },
        None => project::walk(&fallback),
    };

    if sources.is_empty() {
        report::fail("no .pi files to format");
        return EXIT_ERRORS;
    }

    let mut unformatted = Vec::new();
    let mut changed = 0usize;
    for source in &sources {
        let Ok(text) = std::fs::read_to_string(source) else {
            report::fail(format!("cannot read `{}`", source.display()));
            return EXIT_ERRORS;
        };
        let formatted = format::format(&text, source);
        if formatted == text {
            continue;
        }
        if check_only {
            unformatted.push(source.clone());
            continue;
        }
        if let Err(error) = std::fs::write(source, &formatted) {
            report::fail(format!("cannot write `{}`: {error}", source.display()));
            return EXIT_ERRORS;
        }
        println!("{}", project::display(source, &configured.root));
        changed += 1;
    }

    if check_only {
        if unformatted.is_empty() {
            eprintln!("all {} files are formatted", sources.len());
            return EXIT_SUCCESS;
        }
        for path in &unformatted {
            eprintln!("unformatted: {}", project::display(path, &configured.root));
        }
        eprintln!(
            "{} of {} {} not formatted",
            unformatted.len(),
            sources.len(),
            report::plural(unformatted.len(), "file is", "files are")
        );
        return EXIT_ERRORS;
    }

    eprintln!(
        "formatted {changed} of {} {}",
        sources.len(),
        report::plural(sources.len(), "file", "files")
    );
    EXIT_SUCCESS
}

/// `piton format -`: formats stdin to stdout, which is how an editor's
/// external formatter (Vim's `formatprg`, say) calls it.
///
/// A source that does not parse writes nothing to stdout and exits non-zero,
/// so an editor that replaces the buffer with the output leaves it alone.
fn format_stdin(check_only: bool) -> u8 {
    use std::io::{Read, Write};

    let mut source = String::new();
    if let Err(error) = std::io::stdin().read_to_string(&mut source) {
        report::fail(format!("cannot read stdin: {error}"));
        return EXIT_ERRORS;
    }
    let path = std::path::Path::new("<stdin>.pi");
    let formatted = match format::format_checked(&source, path) {
        Ok(formatted) => formatted,
        Err(errors) => {
            let mut sink = piton_core::DiagnosticSink::new();
            sink.extend(errors);
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            report::diagnostics(&sink, &|_| Some(source.clone()), &cwd);
            eprintln!("not formatted; fix the syntax errors above first");
            return EXIT_ERRORS;
        }
    };
    if check_only {
        if formatted == source {
            return EXIT_SUCCESS;
        }
        eprintln!("unformatted: <stdin>");
        return EXIT_ERRORS;
    }
    let mut stdout = std::io::stdout();
    if stdout
        .write_all(formatted.as_bytes())
        .and_then(|()| stdout.flush())
        .is_err()
    {
        return EXIT_ERRORS;
    }
    EXIT_SUCCESS
}
