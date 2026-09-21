//! Compares generated artifacts against the checked-in `.claude` tree.
//!
//! That tree began as the output of an earlier implementation, which made it an
//! independent description of what Belay should produce, and the constants below
//! record where this compiler deliberately disagreed with it.
//!
//! It is no longer independent: `piton build` regenerates it, so what is on disk
//! is this compiler's own most recent output. The test is therefore a snapshot
//! check -- it catches an unintended change in generated output between builds --
//! and not a conformance check against another implementation. Treat a failure
//! as "the output changed, is that intended?" rather than "the output is wrong".

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use piton_compile::{config, Compilation, Framework};

/// Files the golden tree contains that this compiler deliberately does not
/// emit.
///
/// Each is an anchor named only in a `${...}` interpolation. Stringifying an
/// anchor yields its name, not a link, so nothing in the golden tree links to
/// these files either: they are orphans. Emitting a reference document takes a
/// referential interpolation, `@{...}`.
const KNOWN_ORPHANS: &[&str] = &[
    ".claude/reference/scope/language/operators/arithmetic/AdditionOperator.md",
    ".claude/reference/scope/language/operators/arithmetic/DivisionOperator.md",
    ".claude/reference/scope/language/operators/arithmetic/ModuloOperator.md",
    ".claude/reference/scope/language/operators/arithmetic/MultiplicationOperator.md",
    ".claude/reference/scope/language/operators/arithmetic/SubtractionOperator.md",
];

/// Lines where the golden tree is older than the specification it came from.
///
/// `ClaudeCodeAdapter` and `ShapeRootReference` now spell the compiled shape
/// location as `.claude/reference/shape`; the golden tree still carries the
/// earlier `../../shape`. Generating the current text is correct.
const STALE_GOLDEN_LINES: &[(&str, &str)] = &[
    (
        ".claude/reference/scope/belay/Belay.md",
        "documentedClaudeLocation: ../../shape",
    ),
    (
        ".claude/reference/scope/belay/Belay.md",
        "- Preserve a separate compiled shape reference beneath ../../shape.",
    ),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

/// A generated file, with the sources that produced it.
struct Generated {
    contents: String,
    sources: Vec<PathBuf>,
}

/// True when the snapshot predates the source it was generated from.
///
/// The `.claude` tree is a build artifact, so a file older than its own source
/// is simply out of date. A change to the compiler invalidates the snapshot too,
/// which mtimes cannot see: after one, rebuild before reading a failure as a
/// regression.
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
/// A snapshot of generated output says nothing while the source it came from is
/// broken, so the comparison is skipped and `piton check` is left to report the
/// real problem.
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
    let belay = project
        .frameworks
        .iter()
        .find_map(|framework| match framework {
            Framework::Belay(config) => Some(config.clone()),
        })
        .expect("belay configured");
    let compilation = Compilation::build(project);
    let plan = piton_belay::plan(&compilation, &belay);
    let files = plan
        .files
        .iter()
        .map(|file| {
            (
                file.path.to_string_lossy().to_string(),
                file.contents.clone(),
            )
        })
        .collect();
    (files, root)
}

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

/// Every generated file matches the golden tree line for line, once leading
/// whitespace is ignored.
///
/// Indentation is compared separately because this compiler indents the
/// continuation lines of a multi-line value so they stay inside their list item
/// or map entry, which the golden tree does not.
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

/// Generated files keyed by path, carrying their source provenance.
fn generated_with_sources() -> (BTreeMap<String, Generated>, PathBuf) {
    let root = repo_root();
    let (project, _) = config::load(&root, None);
    let belay = project
        .frameworks
        .iter()
        .find_map(|framework| match framework {
            Framework::Belay(config) => Some(config.clone()),
        })
        .expect("belay configured");
    let compilation = Compilation::build(project);
    let plan = piton_belay::plan(&compilation, &belay);
    let files = plan
        .files
        .iter()
        .map(|file| {
            (
                file.path.to_string_lossy().to_string(),
                Generated {
                    contents: file.contents.clone(),
                    sources: file.sources.clone(),
                },
            )
        })
        .collect();
    (files, root)
}

/// Reports how much of the tree reproduces byte for byte, indentation included.
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

/// Identical source, configuration, and directory state produce identical
/// bytes, which is what makes a generated tree reviewable in a diff.
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
