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
    let failed = report(
        &session.compilation.analysis.db,
        &session.compilation.diagnostics.iter().cloned().collect::<Vec<_>>(),
    );
    if failed {
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
    let (outputs, emit_diagnostics) = session.emit();
    let mut diagnostics = session.compilation.diagnostics.iter().cloned().collect::<Vec<_>>();
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
