//! `piton format` — apply the canonical format.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use piton_compile::{Compilation, ModuleId};
use piton_lsp::analysis;
use piton_lsp::index::Index;
use piton_syntax::format;

use anstream::{eprintln, println};

use crate::style::{self, paint};
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

    // Only a project can tell which imports are unused. Without a
    // configuration, the command formats each file alone.
    let imports = configured
        .config_path
        .is_some()
        .then(|| Imports::new(configured.clone(), &sources));

    let mut unformatted = Vec::new();
    let mut changed = 0usize;
    for source in &sources {
        let Ok(text) = std::fs::read_to_string(source) else {
            report::fail(format!("cannot read `{}`", source.display()));
            return EXIT_ERRORS;
        };
        let organized = match &imports {
            Some(imports) => imports.organize(source, &text),
            None => text.clone(),
        };
        let formatted = format::format(&organized, source);
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
        println!("{}", report::added(project::display(source, &configured.root)));
        changed += 1;
    }

    if check_only {
        if unformatted.is_empty() {
            report::done(format!("all {} files are formatted", sources.len()));
            return EXIT_SUCCESS;
        }
        for path in &unformatted {
            eprintln!(
                "{} {}",
                paint(style::WARNING, "unformatted:"),
                project::display(path, &configured.root)
            );
        }
        report::stopped(format!(
            "{} of {} {} not formatted",
            unformatted.len(),
            sources.len(),
            report::plural(unformatted.len(), "file is", "files are")
        ));
        report::help("run `piton format` to format them");
        return EXIT_ERRORS;
    }

    report::done(format!(
        "formatted {changed} of {} {}",
        sources.len(),
        report::plural(sources.len(), "file", "files")
    ));
    EXIT_SUCCESS
}

/// The compilation that tells which imports each file uses.
struct Imports {
    compilation: Compilation,
    index: Index,
    modules: HashMap<PathBuf, ModuleId>,
}

impl Imports {
    /// Compiles every source under the source root, and also `sources`.
    /// Thus a file that no module imports still has its imports checked.
    fn new(project: piton_compile::Project, sources: &[PathBuf]) -> Imports {
        let compilation = Compilation::build_workspace(project, HashMap::new(), sources);
        let index = Index::build(&compilation);
        let modules = compilation
            .graph()
            .iter()
            .filter(|module| !module.is_package())
            .map(|module| (canonical(&module.path), module.id))
            .collect();
        Imports {
            compilation,
            index,
            modules,
        }
    }

    /// `text` without its unused and duplicate imports.
    ///
    /// A file with an error keeps all of its imports. An import that does not
    /// resolve looks unused, but a missing package or an incorrect path can
    /// cause it. The removal of that import loses the intent of the author.
    /// The configuration file also keeps its imports.
    fn organize(&self, source: &Path, text: &str) -> String {
        if piton_lsp::world::is_config_file(source) {
            return text.to_string();
        }
        let path = canonical(source);
        let Some(module) = self.modules.get(&path).copied() else {
            return text.to_string();
        };
        let module_path = &self.compilation.graph().get(module).path;
        if self.compilation.graph().get(module).source != text {
            return text.to_string();
        }
        let broken = self
            .compilation
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.is_error() && diagnostic.file == *module_path);
        if broken {
            return text.to_string();
        }
        let Some(mut edits) =
            analysis::organize_imports(&self.compilation, &self.index, module, text)
        else {
            return text.to_string();
        };
        edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.start));
        let mut out = text.to_string();
        for (span, replacement) in edits {
            out.replace_range(span.start..span.end, &replacement);
        }
        out
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// `piton format -` formats stdin to stdout. An external formatter of an
/// editor calls it in this way. An example is `formatprg` in Vim.
///
/// When a source does not parse, the command writes nothing to stdout. It
/// exits with a non-zero code. Thus an editor that replaces the buffer with
/// the output does not change the buffer.
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
            report::stopped("not formatted; fix the syntax errors below first");
            return EXIT_ERRORS;
        }
    };
    if check_only {
        if formatted == source {
            return EXIT_SUCCESS;
        }
        eprintln!("{} <stdin>", paint(style::WARNING, "unformatted:"));
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
