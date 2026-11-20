//! Evaluation again of only what an edit changes must give the same result
//! as a full evaluation. Each test edits files of a real specbase in several
//! ways. After each edit, it compares the compilation that takes values from
//! the compilation before with a full compilation: each value of each anchor
//! and variable, and each diagnostic.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use piton_compile::module::ParseCache;
use piton_compile::{config, Compilation, Project};

/// The edits, as functions from the text of a file to a new text. `None`
/// when the edit does not apply to the file.
fn edits() -> Vec<(&'static str, fn(&str) -> Option<String>)> {
    vec![
        ("a comment", |text| Some(format!("{text}\n// an edit\n"))),
        ("a word of prose", |text| {
            let at = text.find(": ")? + 2;
            let rest = &text[at..];
            rest.starts_with(|c: char| c.is_alphabetic()).then(|| format!("{}Edited {}", &text[..at], rest))
        }),
        ("an expression", |text| {
            let at = text.find(": ")? + 2;
            let end = at + text[at..].find('\n')?;
            Some(format!("{} {{1 + 1}}{}", &text[..end], &text[end..]))
        }),
        ("an error", |text| {
            let at = text.find(": ")? + 2;
            Some(format!("{}{{NoSuchName.value}} {}", &text[..at], &text[at..]))
        }),
        ("a removed property", |text| {
            let mut lines: Vec<&str> = text.lines().collect();
            let at = lines.iter().position(|line| line.starts_with("    ") && line.trim_start().contains(": "))?;
            lines.remove(at);
            Some(lines.join("\n") + "\n")
        }),
    ]
}

fn compare(label: &str, incremental: &Compilation, full: &Compilation) {
    let (a, b) = (incremental.store(), full.store());
    assert_eq!(a.anchors.len(), b.anchors.len(), "{label}: the anchors differ");
    for (left, right) in a.anchors.iter().zip(&b.anchors) {
        assert_eq!(left.name, right.name, "{label}: the anchors differ");
        assert_eq!(left.properties, right.properties, "{label}: the values of `{}` differ", left.name);
    }
    for (left, right) in a.variables.iter().zip(&b.variables) {
        assert_eq!(left.value, right.value, "{label}: the value of `{}` differs", left.name);
    }
    let describe = |compilation: &Compilation| {
        let mut out: Vec<String> = compilation
            .diagnostics
            .iter()
            .map(|d| format!("{} {:?} {} {}", d.file.display(), d.span, d.code, d.message))
            .collect();
        out.sort();
        out.dedup();
        out
    };
    assert_eq!(describe(incremental), describe(full), "{label}: the diagnostics differ");
}

/// Edits each `step`-th source file of the project at `root` in each way,
/// one edit after the other, and compares each compilation.
fn check(root: &Path, step: usize) -> usize {
    let (project, _) = config::load(root, None);
    let cache = Arc::new(ParseCache::default());
    let build = |project: Project, buffers: &HashMap<PathBuf, String>, previous: Option<&Compilation>| {
        let also: Vec<PathBuf> = buffers.keys().cloned().collect();
        Compilation::build_workspace_cached(project, buffers.clone(), &also, &cache, previous)
    };
    let mut files = piton_compile::module::sources(&project.source_root);
    files.sort();
    let mut previous = build(project.clone(), &HashMap::new(), None);
    let mut reused = 0;
    for file in files.iter().step_by(step) {
        let original = std::fs::read_to_string(file).expect("readable");
        let mut text = original.clone();
        for (name, edit) in edits() {
            let Some(edited) = edit(&text) else { continue };
            text = edited;
            let buffers = HashMap::from([(file.clone(), text.clone())]);
            let incremental = build(project.clone(), &buffers, Some(&previous));
            let full = Compilation::build_workspace(project.clone(), buffers.clone(), &[file.clone()]);
            compare(&format!("{} after {name}", file.display()), &incremental, &full);
            reused += usize::from(incremental.reevaluated.is_some());
            previous = incremental;
        }
        // Back to the text on disk, which the next file starts from.
        let incremental = build(project.clone(), &HashMap::from([(file.clone(), original.clone())]), Some(&previous));
        let full = Compilation::build_workspace(project.clone(), HashMap::from([(file.clone(), original)]), &[file.clone()]);
        compare(&format!("{} restored", file.display()), &incremental, &full);
        previous = incremental;
    }
    reused
}

#[test]
fn edits_to_the_specification_evaluate_the_same_as_a_full_compilation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let reused = check(&root, 25);
    assert!(reused > 0, "no edit took values from the compilation before");
}

/// The same check for another project, in a release build:
/// `PITON_INCREMENTAL_PROJECT=<dir> cargo test --release -p piton-compile
/// --test incremental -- --ignored`.
#[test]
#[ignore]
fn edits_to_another_project_evaluate_the_same_as_a_full_compilation() {
    let Some(root) = std::env::var_os("PITON_INCREMENTAL_PROJECT") else {
        return;
    };
    let step: usize = std::env::var("PITON_INCREMENTAL_STEP").ok().and_then(|n| n.parse().ok()).unwrap_or(40);
    let reused = check(Path::new(&root), step);
    eprintln!("{reused} compilations took values from the compilation before");
}

/// A project in which items of one file depend on items of another, in each
/// way that the evaluation knows.
const DEPENDENCIES: &[(&str, &str)] = &[
    ("piton.config.pi", "use @piton/config\n\nexport piton-config Deps:\n    root: ./spec\n    entry: ./spec/index.pi\n"),
    (
        "spec/a.pi",
        "export LIMIT: 3\n\nexport anchor A:\n    x: first\n    count: 2\n    greeting: Hello from ${self.label}\n    label: A\n\nexport abstract anchor Shape as shape:\n    size:: number\n",
    ),
    (
        "spec/b.pi",
        "from ./a import A, LIMIT\n\nexport anchor B:\n    copy: {A.x}\n    text: Copy of ${A.x}\n    total: {A.count + LIMIT}\n    limit: Limit ${LIMIT}\n",
    ),
    (
        "spec/c.pi",
        "use ./a\n\nfrom ./a import A\n\nexport anchor C extends A:\n    label: C\n    more: ${super.x} and more\n\nexport shape Square:\n    size: 4\n",
    ),
    ("spec/d.pi", "from ./index import B\n\nexport anchor D:\n    seen: {B.copy}\n"),
    // E depends on A only through what it inherits.
    ("spec/e.pi", "from ./a import A\n\nexport anchor E extends A:\n    label: E\n"),
    (
        "spec/index.pi",
        "from ./a export A\nfrom ./b export B\nfrom ./c export C, Square\nfrom ./d export D\nfrom ./e export E\n",
    ),
];

/// Edits, each of a file and from its text before.
const EDITS: &[(&str, &str, &str)] = &[
    ("spec/a.pi", "    x: first\n", "    x: second\n"),
    ("spec/a.pi", "    count: 2\n", "    count: 5\n"),
    ("spec/a.pi", "export LIMIT: 3\n", "export LIMIT: 7\n"),
    ("spec/a.pi", "    greeting: Hello from ${self.label}\n", "    greeting: Hi from ${self.label}\n"),
    ("spec/a.pi", "    x: second\n", "    x: {NoSuchName.value}\n"),
    ("spec/a.pi", "    x: {NoSuchName.value}\n", "    x: third\n"),
    ("spec/a.pi", "    size:: number\n", "    size:: string\n"),
    ("spec/a.pi", "    size:: string\n", "    size:: number\n"),
    ("spec/a.pi", "export anchor A:\n", "anchor A:\n"),
    ("spec/a.pi", "anchor A:\n", "export anchor A:\n"),
    // The index now exports C under the name B. No declaration changes.
    ("spec/index.pi", "from ./b export B\n", "from ./c export C B\n"),
    ("spec/a.pi", "    greeting: Hi from ${self.label}\n", "    greeting: Hey from ${self.label}\n"),
    // A property that goes, and comes back, and one that is new.
    ("spec/a.pi", "    label: A\n", ""),
    ("spec/a.pi", "    count: 5\n", "    count: 5\n    label: A\n"),
    ("spec/b.pi", "    limit: Limit ${LIMIT}\n", "    limit: Limit ${LIMIT}\n    extra: {A.label}\n"),
    ("spec/c.pi", "    more: ${super.x} and more\n", ""),
];

#[test]
fn edits_that_other_files_depend_on_evaluate_the_same_as_a_full_compilation() {
    let root = std::env::temp_dir().join(format!("piton-incremental-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in DEPENDENCIES {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let (project, _) = config::load(&root, None);
    let cache = Arc::new(ParseCache::default());
    let mut buffers: HashMap<PathBuf, String> = HashMap::new();
    let mut previous = Compilation::build_workspace_cached(project.clone(), HashMap::new(), &[], &cache, None);
    let mut reused = 0;
    for (path, from, to) in EDITS {
        let file = root.join(path);
        let text = buffers.get(&file).cloned().unwrap_or_else(|| std::fs::read_to_string(&file).unwrap());
        assert!(text.contains(from), "the edit `{from}` does not apply to {path}");
        buffers.insert(file.clone(), text.replacen(from, to, 1));
        let also: Vec<PathBuf> = buffers.keys().cloned().collect();
        let incremental =
            Compilation::build_workspace_cached(project.clone(), buffers.clone(), &also, &cache, Some(&previous));
        let full = Compilation::build_workspace(project.clone(), buffers.clone(), &also);
        compare(&format!("after `{}` in {path}", to.trim()), &incremental, &full);
        reused += usize::from(incremental.reevaluated.is_some());
        previous = incremental;
    }
    let _ = std::fs::remove_dir_all(&root);
    assert!(reused >= 8, "only {reused} edits took values from the compilation before");
}
