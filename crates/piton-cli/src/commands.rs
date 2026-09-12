//! The behaviour behind each `piton` subcommand.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use piton_core::serialize::{to_json_string, to_yaml};
use piton_core::value::Value;
use piton_core::FileId;

use crate::files;
use crate::report::report;
use crate::session::{registry, Session};
use piton_core::project::Project;

/// What `piton compile` writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Json,
    Yaml,
}

impl Format {
    fn extension(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Yaml => "yaml",
        }
    }

    fn render(self, value: &Value) -> String {
        match self {
            Format::Json => format!("{}\n", to_json_string(value, true)),
            Format::Yaml => to_yaml(value),
        }
    }
}

/// `piton compile` and `piton check`.
pub fn compile(
    patterns: &[String],
    format: Format,
    out_dir: Option<&Path>,
    to_stdout: bool,
    write: bool,
) -> Result<i32> {
    let paths = files::resolve(patterns)?;
    let cwd = std::env::current_dir()?;
    let session = Session::files(&cwd, &paths)?;
    if let Some(code) = misconfigured(&session) {
        return Ok(code);
    }
    if report(&session.compilation.analysis.db, &session.diagnostics()) {
        return Ok(1);
    }
    if !write {
        eprintln!("checked {} file{}", paths.len(), if paths.len() == 1 { "" } else { "s" });
        return Ok(0);
    }

    for path in &paths {
        let Some(file) = session.compilation.analysis.db.file_id(path) else { continue };
        let value = document(&session, file);
        let rendered = format.render(&value);
        if to_stdout {
            print!("{rendered}");
            continue;
        }
        let target = match out_dir {
            Some(dir) => dir.join(path.file_name().unwrap_or_default()).with_extension(format.extension()),
            None => path.with_extension(format.extension()),
        };
        write_file(&target, &rendered)?;
        println!("{}", target.display());
    }
    Ok(0)
}

/// The whole file as one dictionary of its top-level names.
fn document(session: &Session, file: piton_core::FileId) -> Value {
    let mut dict = piton_core::value::Dict::new();
    for (name, value) in session.compilation.file_values(file) {
        dict.insert(name, value);
    }
    Value::Dict(dict)
}

/// `piton build` and `piton build check`.
pub fn build(check_only: bool) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let session = Session::project(&cwd)?;
    for note in &session.notes {
        eprintln!("warning[framework]: {note}");
    }
    if let Some(code) = misconfigured(&session) {
        return Ok(code);
    }
    let (outputs, emit_diagnostics) = session.emit();
    let mut diagnostics = session.diagnostics();
    diagnostics.extend(emit_diagnostics);
    if report(&session.compilation.analysis.db, &diagnostics) {
        return Ok(1);
    }
    if check_only {
        eprintln!("checked {} output file{}", outputs.len(), if outputs.len() == 1 { "" } else { "s" });
        return Ok(0);
    }
    for output in &outputs {
        write_file(&output.path, &output.contents)?;
        println!("{}", output.path.display());
    }
    eprintln!("wrote {} file{}", outputs.len(), if outputs.len() == 1 { "" } else { "s" });
    Ok(0)
}

/// `piton reach`: which files under the project root are compiled, and which
/// are not.
///
/// Only files reachable from the entry point are compiled at all, so an
/// unreached file is invisible: its errors are never reported and nothing it
/// declares exists. That is easy to do by accident and hard to notice.
pub fn reach(
    target: Option<PathBuf>,
    show: Reach,
    strict: bool,
    chains: bool,
    entry: Option<PathBuf>,
) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let session = match &entry {
        Some(path) => Session::files(&cwd, std::slice::from_ref(path))?,
        None => {
            let session = Session::project(&cwd)?;
            if let Some(code) = misconfigured(&session) {
                return Ok(code);
            }
            // Without a config there is no entry point, and "reached" is
            // measured from one. Answering anyway would mean treating every
            // file as its own entry and reporting that everything is reached,
            // which is true and useless.
            if session.project.config_path.is_none() {
                bail!(
                    "reachability is measured from an entry point, and there is no \
                     piton.config.pi here.\n  Run this from a project directory, or name \
                     one file to measure from:\n    piton reach --entry spec/index.pi"
                );
            }
            session
        }
    };
    let compilation = &session.compilation;
    let root = &session.project.root;

    // Asking about one file is a different question: not "what is reached" but
    // "how did this get here", or "why did it not".
    if let Some(target) = &target {
        return explain(&session, target);
    }

    let under_root: Vec<PathBuf> = files::walk(root);
    let unreached: Vec<String> = under_root
        .iter()
        .filter(|path| !compilation.reaches(path))
        .map(|path| relative(path, root))
        .collect();
    let reached = under_root.len() - unreached.len();

    // Say what the answer is measured from, so it cannot be misread.
    let entries: Vec<String> = compilation
        .entries
        .iter()
        .filter_map(|file| compilation.analysis.db.file(*file).source.as_path())
        .map(|path| relative(path, root))
        .collect();
    match entries.len() {
        1 => eprintln!("entry point: {}", entries[0]),
        count => eprintln!(
            "warning: {count} entry points — every file under {} is its own, so everything \
             is trivially reached.\n  Give the project an `entry` in piton.config.pi, or an \
             index.pi at its root.",
            root.display()
        ),
    }

    if show != Reach::Unreached {
        println!("reached ({reached})");
        if chains {
            print_chains(&session, root);
        } else {
            print_reach_tree(&session, root);
        }
    }
    if show != Reach::Reached {
        if show == Reach::All && reached > 0 && !unreached.is_empty() {
            println!();
        }
        println!("unreached ({})", unreached.len());
        for path in &unreached {
            println!("  {path}");
        }
    }

    if show == Reach::All {
        eprintln!(
            "\n{reached} of {} file(s) reached from the entry point",
            under_root.len()
        );
        if !unreached.is_empty() {
            eprintln!("an unreached file is never compiled, so its errors are never reported");
        }
    }
    Ok(i32::from(strict && !unreached.is_empty()))
}

/// Explain how one file is reached, or why it is not.
fn explain(session: &Session, target: &Path) -> Result<i32> {
    let compilation = &session.compilation;
    let root = &session.project.root;
    let absolute = piton_core::db::canonical(target);
    let shown = relative(&absolute, root);

    let Some(file) = compilation.analysis.db.file_id(&absolute) else {
        return explain_unreached(session, &absolute, &shown);
    };

    let steps = compilation.reach_steps(file);
    if steps.is_empty() {
        println!("{shown} is the entry point");
        return Ok(0);
    }

    println!("{shown} is reached by {} import(s):\n", steps.len());
    for step in &steps {
        let source = compilation.analysis.db.file(step.from);
        let (line, _) = crate::report::position(&source.text, step.range);
        let from = source
            .source
            .as_path()
            .map(|path| relative(path, root))
            .unwrap_or_else(|| source.source.display());
        println!("  {from}:{line}");
        println!("      {}  ->  {}", step.specifier, label(session, step.to, root));
    }
    Ok(0)
}

/// A file that was not reached: say what would have had to import it.
fn explain_unreached(session: &Session, absolute: &Path, shown: &str) -> Result<i32> {
    let root = &session.project.root;
    if !absolute.is_file() {
        bail!("no such file: {}", absolute.display());
    }
    println!("{shown} is NOT reached, so it is never compiled.\n");

    // The file is not in this compilation, so nothing here can name it. Load
    // the whole tree to find out who imports it, and whether they are reached.
    let cwd = std::env::current_dir()?;
    let everything = files::walk(root);
    let full = Session::files(&cwd, &everything)?;
    let Some(file) = full.compilation.analysis.db.file_id(absolute) else {
        bail!("{shown} could not be loaded");
    };

    let importers = full.compilation.importers(file);
    if importers.is_empty() {
        println!("  Nothing imports it. Add it to an index.pi, or import it where it is needed.");
        return Ok(0);
    }

    println!("  It is imported by, none of which is reached either:\n");
    for step in &importers {
        let source = full.compilation.analysis.db.file(step.from);
        let Some(path) = source.source.as_path() else { continue };
        let (line, _) = crate::report::position(&source.text, step.range);
        let reached = session.compilation.reaches(path);
        println!(
            "  {}:{line}    {}{}",
            relative(path, root),
            step.specifier,
            if reached { "   (reached — this is a bug, please report it)" } else { "" }
        );
    }
    println!("\n  Follow one of those up with `piton reach <that file>`.");
    Ok(0)
}

fn label(session: &Session, file: FileId, root: &Path) -> String {
    let source = &session.compilation.analysis.db.file(file).source;
    match source.as_path() {
        Some(path) => relative(path, root),
        None => source.display(),
    }
}

/// Each reached file under the one that pulled it in, so the indentation is
/// the chain read downwards.
fn print_reach_tree(session: &Session, root: &Path) {
    let compilation = &session.compilation;
    let parents = compilation.reached_by();
    let mut children: BTreeMap<Option<FileId>, Vec<FileId>> = BTreeMap::new();
    for (file, parent) in &parents {
        children.entry(*parent).or_default().push(*file);
    }
    let label = |file: FileId| match compilation.analysis.db.file(file).source.as_path() {
        Some(path) => relative(path, root),
        None => compilation.analysis.db.file(file).source.display(),
    };
    for list in children.values_mut() {
        list.sort_by_key(|file| label(*file));
    }

    fn walk(
        parent: Option<FileId>,
        depth: usize,
        children: &BTreeMap<Option<FileId>, Vec<FileId>>,
        label: &dyn Fn(FileId) -> String,
        root: &Path,
        session: &Session,
    ) {
        let Some(list) = children.get(&parent) else { return };
        for file in list {
            // Builtin modules are reached but are not files anyone can open.
            if session.compilation.analysis.db.file(*file).source.as_path().is_none() {
                continue;
            }
            let _ = root;
            println!("{}{}", "  ".repeat(depth + 1), label(*file));
            walk(Some(*file), depth + 1, children, label, root, session);
        }
    }
    walk(None, 0, &children, &label, root, session);
}

/// One explicit `a -> b -> c` per reached file, for copying out.
fn print_chains(session: &Session, root: &Path) {
    let compilation = &session.compilation;
    let label = |file: FileId| match compilation.analysis.db.file(file).source.as_path() {
        Some(path) => relative(path, root),
        None => compilation.analysis.db.file(file).source.display(),
    };
    let mut lines: Vec<String> = Vec::new();
    for file in compilation.analysis.db.files() {
        if file.source.as_path().is_none() {
            continue;
        }
        let chain = compilation.reach_chain(file.id);
        if chain.is_empty() {
            continue;
        }
        let rendered: Vec<String> = chain.iter().map(|it| label(*it)).collect();
        lines.push(match rendered.len() {
            1 => format!("  {}  (entry point)", rendered[0]),
            _ => format!("  {}", rendered.join(" -> ")),
        });
    }
    lines.sort();
    for line in lines {
        println!("{line}");
    }
}

/// Which half of the answer to print.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Reach {
    All,
    Reached,
    Unreached,
}

/// A path shown relative to the project root, for readability.
fn relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).display().to_string()
}

/// `piton format`.
pub fn format(patterns: &[String], check_only: bool) -> Result<i32> {
    if patterns.len() == 1 && patterns[0] == "-" {
        let mut source = String::new();
        std::io::stdin().read_to_string(&mut source)?;
        print!("{}", piton_fmt::format(&source));
        return Ok(0);
    }
    let paths = files::resolve(patterns)?;
    let mut changed = Vec::new();
    for path in &paths {
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let formatted = piton_fmt::format(&source);
        if formatted == source {
            continue;
        }
        changed.push(path.clone());
        if !check_only {
            std::fs::write(path, &formatted)
                .with_context(|| format!("writing {}", path.display()))?;
        }
    }
    for path in &changed {
        println!("{}", path.display());
    }
    if check_only && !changed.is_empty() {
        eprintln!("{} file(s) are not formatted", changed.len());
        return Ok(1);
    }
    Ok(0)
}

/// `piton loc`: how many lines, and of what.
///
/// With no argument it counts the whole project. Prose and structure are
/// reported separately, because in a language written mostly in prose a single
/// number says very little.
pub fn loc(patterns: &[String], by_file: bool, reached_only: bool) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let paths = if patterns.is_empty() {
        project_files(&cwd, reached_only)?
    } else {
        if reached_only {
            bail!("--reached counts the project; it cannot be combined with a path");
        }
        files::resolve(patterns)?
    };

    let mut totals = piton_syntax::loc::Counts::default();
    let mut rows: Vec<(piton_syntax::loc::Counts, String)> = Vec::new();
    for path in &paths {
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let counts = piton_syntax::loc::count(&source);
        totals.add(counts);
        rows.push((counts, display_path(path, &cwd)));
    }

    if by_file {
        // Biggest first: the question behind a per-file count is usually
        // "what is the large one".
        rows.sort_by(|a, b| b.0.total.cmp(&a.0.total).then_with(|| a.1.cmp(&b.1)));
        println!("{}", loc_header("file"));
        for (counts, name) in &rows {
            println!("{}", loc_row(*counts, name));
        }
        println!();
    }

    println!("{}", loc_header("files"));
    println!("{}", loc_row(totals, &paths.len().to_string()));
    Ok(0)
}

/// The `.pi` files of the project in `cwd`.
/// The heading above a line count. The last column names what the rows are.
fn loc_header(last: &str) -> String {
    format!("{:>7} {:>6} {:>6} {:>8} {:>6}  {last}", "lines", "code", "prose", "comment", "blank")
}

/// One row of a line count, laid out under `loc_header`.
fn loc_row(counts: piton_syntax::loc::Counts, last: &str) -> String {
    format!(
        "{:>7} {:>6} {:>6} {:>8} {:>6}  {last}",
        counts.total, counts.code, counts.prose, counts.comment, counts.blank
    )
}

/// Report a configuration that cannot be honoured, and say so with an exit code.
///
/// Every command asks this before it uses a session. A project that has not
/// finished saying where its code lives has nothing reliable to say about the
/// code, and the one line the author can fix should not arrive underneath every
/// import it broke. This is the language server's rule too, so the editor and
/// the compiler describe a broken configuration the same way.
fn misconfigured(session: &Session) -> Option<i32> {
    if !session.misconfigured {
        return None;
    }
    report(&session.compilation.analysis.db, &session.diagnostics());
    Some(1)
}

fn project_files(cwd: &Path, reached_only: bool) -> Result<Vec<PathBuf>> {
    let loaded = Project::load(cwd, &registry());
    let root = if loaded.project.config_path.is_some() { loaded.project.root.clone() } else { cwd.to_path_buf() };
    let all = files::walk(&root);
    if all.is_empty() {
        bail!("no .pi files under {}", root.display());
    }
    if !reached_only {
        return Ok(all);
    }
    if loaded.project.config_path.is_none() {
        bail!("--reached needs a piton.config.pi, because reachability starts at an entry point");
    }
    let session = Session::project(cwd)?;
    if session.misconfigured {
        report(&session.compilation.analysis.db, &session.diagnostics());
        bail!("reachability is measured from an entry point, and this project has no usable one");
    }
    Ok(all.into_iter().filter(|path| session.compilation.reaches(path)).collect())
}

/// A path relative to the working directory when that is shorter.
fn display_path(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd).unwrap_or(path).display().to_string()
}

/// `piton grammar`: write every editor integration.
pub fn grammar(out_dir: &Path, repository: Option<String>, rev: Option<String>) -> Result<i32> {
    let frameworks = registry();
    let docs = piton_docs::describe(&frameworks);
    let keywords: Vec<String> = docs.iter().flat_map(|doc| doc.keywords()).collect();
    let sigils: Vec<String> = docs.iter().flat_map(|doc| doc.sigils.clone()).collect();
    let vocabulary =
        piton_grammar::Vocabulary::from_compiler().with_framework(keywords, sigils);

    // A published grammar commit cannot be regenerated, so an existing pin is
    // carried forward unless the caller supplies a new one. An unpinned file
    // holds nothing worth keeping.
    let existing = published_grammar(out_dir).filter(piton_grammar::GrammarSource::is_published);
    let source = piton_grammar::GrammarSource {
        repository: repository
            .or_else(|| existing.as_ref().map(|it| it.repository.clone()))
            .unwrap_or(piton_grammar::GrammarSource::default().repository),
        rev: rev
            .or_else(|| existing.as_ref().map(|it| it.rev.clone()))
            .unwrap_or(piton_grammar::GrammarSource::default().rev),
    };

    let generated = piton_grammar::generate(&vocabulary, &source);
    for file in &generated {
        write_file(&out_dir.join(&file.path), &file.contents)?;
    }
    println!("wrote {} files to {}", generated.len(), out_dir.display());
    if !source.is_published() {
        println!(
            "note: the Tree-sitter grammar has no home yet, so Zed and Helix cannot \
             fetch it.\n      git remote add {} <url>\n      cargo xtask publish-grammar",
            piton_grammar::GRAMMAR_REMOTE_NAME
        );
    }
    Ok(0)
}

/// Read the grammar pin back out of a previously generated Zed manifest.
fn published_grammar(out_dir: &Path) -> Option<piton_grammar::GrammarSource> {
    let manifest = std::fs::read_to_string(out_dir.join("zed").join("extension.toml")).ok()?;
    let grammars = manifest.split("[grammars.piton]").nth(1)?;
    let field = |name: &str| {
        grammars
            .lines()
            .find_map(|line| line.trim().strip_prefix(name))
            .and_then(|rest| rest.split('"').nth(1))
            .map(str::to_string)
    };
    Some(piton_grammar::GrammarSource {
        repository: field("repository = ")?,
        rev: field("commit = ")?,
    })
}

/// `piton docs`: the generated language reference.
pub fn docs(out: Option<&Path>) -> Result<i32> {
    let frameworks = registry();
    let reference = piton_docs::language_reference(&frameworks);
    match out {
        Some(path) => {
            write_file(path, &reference)?;
            println!("{}", path.display());
        }
        None => print!("{reference}"),
    }
    Ok(0)
}

/// `piton ast`: dump the concrete syntax tree, for debugging the compiler.
pub fn ast(path: &Path) -> Result<i32> {
    let source = std::fs::read_to_string(path)?;
    let parse = piton_syntax::parse(&source);
    print_tree(&parse.syntax(), 0);
    for error in &parse.errors {
        let (line, column) = crate::report::position(&source, error.range);
        eprintln!("error at {line}:{column}: {}", error.message);
    }
    Ok(0)
}

fn print_tree(node: &piton_syntax::SyntaxNode, depth: usize) {
    println!("{:indent$}{:?}", "", node, indent = depth * 2);
    for child in node.children_with_tokens() {
        match child {
            piton_syntax::NodeOrToken::Node(child) => print_tree(&child, depth + 1),
            piton_syntax::NodeOrToken::Token(token) => println!(
                "{:indent$}{:?} {:?}",
                "",
                token.kind(),
                token.text(),
                indent = (depth + 1) * 2
            ),
        }
    }
}

pub fn write_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut file = std::fs::File::create(path)
        .with_context(|| format!("writing {}", path.display()))?;
    file.write_all(contents.as_bytes())?;
    Ok(())
}

/// `piton claude`: start the Claude CLI already fluent in Piton.
pub fn claude(args: &[String], print_prompt: bool, install: bool) -> Result<i32> {
    let frameworks = registry();
    let brief = piton_docs::fluency_brief(&frameworks);

    if print_prompt {
        print!("{brief}");
        return Ok(0);
    }
    if install {
        let path = PathBuf::from(".claude/skills/piton/SKILL.md");
        write_file(&path, &piton_docs::skill_document(&frameworks))?;
        println!("{}", path.display());
        return Ok(0);
    }

    let mut command = std::process::Command::new("claude");
    command.arg("--append-system-prompt").arg(&brief);
    command.args(args);
    match command.status() {
        Ok(status) => Ok(status.code().unwrap_or(1)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            bail!(
                "could not find the `claude` executable on PATH.\n\
                 Install Claude Code, or run `piton claude --print-prompt` to get the brief \
                 and paste it in yourself."
            )
        }
        Err(error) => Err(error.into()),
    }
}
