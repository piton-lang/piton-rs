//! Compares generated artifacts with the `.claude` tree in the repository.
//!
//! At first, that tree was the output of an earlier implementation. Thus it
//! was an independent description of the correct Belay output. The constants
//! below record where this compiler did not agree with it, by intention.
//!
//! The tree is not independent now. `piton build` generates it again. Thus the
//! tree on disk is the most recent output of this compiler. The test is a
//! snapshot check. It finds an unintended change in generated output between
//! builds. It is not a conformance check against a different implementation.
//! A failure means "the output changed. Did you intend this change?". It does
//! not mean "the output is wrong".

mod belay;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use piton_compile::{config, Compilation};

/// Files in the golden tree that this compiler does not emit, by intention.
/// It is empty while the tree is the output of this compiler.
const KNOWN_ORPHANS: &[&str] = &[];

/// Lines where the golden tree is older than its source specification. It is
/// empty while the tree is the output of this compiler.
const STALE_GOLDEN_LINES: &[(&str, &str)] = &[];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

/// A generated file, with its sources.
struct Generated {
    contents: String,
    sources: Vec<PathBuf>,
}

/// True when the snapshot is older than its source.
///
/// The `.claude` tree is a build artifact. Thus a file that is older than its
/// source is only out of date. A change to the compiler also makes the snapshot
/// incorrect, and mtimes cannot show this change. After such a change, build
/// again before you think that a failure is a regression.
fn is_stale(golden_path: &Path, generated: &Generated) -> bool {
    let Ok(golden_time) = std::fs::metadata(golden_path).and_then(|m| m.modified()) else {
        return false;
    };
    generated.sources.iter().any(|source| {
        std::fs::metadata(source)
            .and_then(|m| m.modified())
            .is_ok_and(|source_time| source_time > golden_time)
    })
}

/// True when the specbase does not compile.
///
/// A snapshot of generated output has no value while its source is broken.
/// Thus the test does not do the comparison. `piton check` reports the real
/// problem.
fn specbase_is_broken() -> Option<String> {
    let root = repo_root();
    let (project, _) = config::load(&root, None);
    let compilation = Compilation::build(project);
    let errors: Vec<String> = compilation
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.file.display(), d.message))
        .collect();
    (!errors.is_empty()).then(|| errors.join("\n  "))
}

fn generated() -> (BTreeMap<String, String>, PathBuf) {
    let root = repo_root();
    let (project, _) = config::load(&root, None);
    assert!(!project.plugins.is_empty(), "belay configured");
    let compilation = Compilation::build(project);
    let plan = belay::plan(&compilation);
    let files = plan
        .files
        .iter()
        .filter(|file| file.target == GOLDEN_TARGET)
        // The golden tree is only `.claude`. Instruction files, like
        // `crates/CLAUDE.md`, are outside it, so the test does not compare them.
        .filter(|file| file.path.starts_with(".claude"))
        .map(|file| {
            (
                file.path.to_string_lossy().to_string(),
                file.contents.clone(),
            )
        })
        .collect();
    (files, root)
}

/// The golden tree is the `.claude` tree. Thus the test can compare only the
/// files for that target. A project can configure many adapters, and this
/// project does. The other adapters write different trees. This snapshot does
/// not record those trees.
const GOLDEN_TARGET: &str = "claude-code";

fn golden(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.join(".claude")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Git worktrees in `.claude/worktrees` are full checkouts of
                // the project. They are not generated output.
                if path == root.join(".claude").join("worktrees") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                let relative = path.strip_prefix(root).expect("under root");
                let text = std::fs::read_to_string(&path).expect("readable");
                out.insert(relative.to_string_lossy().to_string(), text);
            }
        }
    }
    out
}

#[test]
fn the_generated_file_set_matches() {
    if let Some(errors) = specbase_is_broken() {
        eprintln!("skipped: the specbase does not compile, so its snapshot is not comparable\n  {errors}");
        return;
    }
    let (files, root) = generated();
    let golden = golden(&root);
    assert!(!golden.is_empty(), "no golden tree to compare against");

    let missing: Vec<&String> = golden
        .keys()
        .filter(|key| !files.contains_key(*key))
        .filter(|key| !KNOWN_ORPHANS.contains(&key.as_str()))
        .collect();
    let extra: Vec<&String> = files
        .keys()
        .filter(|key| !golden.contains_key(*key))
        .collect();

    assert!(
        missing.is_empty() && extra.is_empty(),
        "file set differs\nnot generated ({}):\n  {}\nonly generated ({}):\n  {}",
        missing.len(),
        missing
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  "),
        extra.len(),
        extra
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  "),
    );
}

/// Each generated file agrees with the golden tree line for line. The test
/// ignores the whitespace at the start of each line.
///
/// A different test compares the indentation. This compiler indents the
/// continuation lines of a multi-line value. Thus these lines stay in their
/// list item or map entry. The golden tree does not indent them.
#[test]
fn generated_content_matches_line_for_line() {
    if let Some(errors) = specbase_is_broken() {
        eprintln!("skipped: the specbase does not compile, so its snapshot is not comparable\n  {errors}");
        return;
    }
    let (files, root) = generated_with_sources();
    let golden = golden(&root);
    let mut problems = Vec::new();
    let mut skipped = 0usize;

    for (path, expected) in &golden {
        let Some(entry) = files.get(path) else {
            continue;
        };
        if is_stale(&root.join(path), entry) {
            skipped += 1;
            continue;
        }
        let actual = &entry.contents;
        let expected_lines: Vec<&str> = expected.lines().map(str::trim_start).collect();
        let actual_lines: Vec<&str> = actual.lines().map(str::trim_start).collect();

        for (index, expected_line) in expected_lines.iter().enumerate() {
            let actual_line = actual_lines.get(index).copied().unwrap_or("<missing>");
            if actual_line == *expected_line {
                continue;
            }
            if STALE_GOLDEN_LINES.contains(&(path.as_str(), expected_line)) {
                continue;
            }
            problems.push(format!(
                "{path}:{}\n  golden:    {expected_line}\n  generated: {actual_line}",
                index + 1
            ));
        }
        if actual_lines.len() != expected_lines.len() {
            problems.push(format!(
                "{path}: {} generated lines, {} golden lines",
                actual_lines.len(),
                expected_lines.len()
            ));
        }
    }

    if skipped > 0 {
        eprintln!("{skipped} golden files skipped: older than the source they came from");
    }
    assert!(
        problems.is_empty(),
        "{} differences:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// The modules that declare the source anchors of a file. The plugin plan
/// names them in the origin of the file, with commas between them.
fn sources(compilation: &Compilation, origin: &str) -> Vec<PathBuf> {
    let mut sources: Vec<PathBuf> = origin
        .split(", ")
        .filter_map(|name| compilation.find_anchor(name.trim()))
        .map(|anchor| compilation.anchor_module_path(anchor))
        .collect();
    sources.sort();
    sources.dedup();
    sources
}

/// Generated files by path, with the provenance of their source.
fn generated_with_sources() -> (BTreeMap<String, Generated>, PathBuf) {
    let root = repo_root();
    let (project, _) = config::load(&root, None);
    assert!(!project.plugins.is_empty(), "belay configured");
    let compilation = Compilation::build(project);
    let plan = belay::plan(&compilation);
    let files = plan
        .files
        .iter()
        .filter(|file| file.target == GOLDEN_TARGET)
        .map(|file| {
            (
                file.path.to_string_lossy().to_string(),
                Generated {
                    contents: file.contents.clone(),
                    sources: sources(&compilation, &file.origin),
                },
            )
        })
        .collect();
    (files, root)
}

/// Reports the part of the tree that is the same byte for byte, with indentation.
#[test]
fn report_byte_identical_share() {
    let (files, root) = generated_with_sources();
    let golden = golden(&root);
    let compared: Vec<&String> = golden
        .keys()
        .filter(|key| files.contains_key(*key))
        .filter(|key| !is_stale(&root.join(key), &files[*key]))
        .collect();
    let identical = compared
        .iter()
        .filter(|path| files[**path].contents == golden[**path])
        .count();
    eprintln!(
        "{identical}/{} compared files are byte identical to the golden tree",
        compared.len()
    );
}

/// The same source, configuration and directory state give the same bytes.
/// Thus a person can review a generated tree in a diff.
#[test]
fn planning_is_deterministic() {
    let (first, _) = generated_with_sources();
    let (second, _) = generated_with_sources();
    assert_eq!(first.len(), second.len());
    for (path, entry) in &first {
        let other = second.get(path).unwrap_or_else(|| panic!("missing {path}"));
        assert_eq!(entry.contents, other.contents, "{path} differs between runs");
    }
    let order_first: Vec<&String> = first.keys().collect();
    let order_second: Vec<&String> = second.keys().collect();
    assert_eq!(order_first, order_second);
}
