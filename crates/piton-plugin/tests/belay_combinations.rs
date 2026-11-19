//! Records the Belay output for each combination of the built-in adapters.
//!
//! The project is the application with Belay that `piton init` starts. It has an
//! instruction, a skill, a command, an agent and references. Thus each
//! combination examines what the tools share. The tools share one reference
//! tree and one AGENTS.md. A tool also finds skills in the directory of
//! another tool.
//!
//! The test compares the plan of each combination with
//! `tests/belay_snapshots/<combination>.txt`. The plan is each file with its
//! contents, then each diagnostic. After an intended change, set
//! `BELAY_UPDATE_SNAPSHOTS=1` to write the snapshots again. Then read the diff.

mod belay;

use std::path::{Path, PathBuf};

use piton_compile::{config, Compilation};

const ADAPTERS: &[(&str, &str)] = &[
    ("claude-code", "ClaudeCodeAdapter"),
    ("codex", "CodexAdapter"),
    ("opencode", "OpenCodeAdapter"),
    ("muse", "MuseAdapter"),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

/// Copies `from` into `to`, with the placeholders of `piton init` filled in
/// as it fills them for an application named `app` with Belay's default
/// roots.
fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("directory");
    for entry in std::fs::read_dir(from).expect("part").flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name == ".adapters" {
            continue;
        }
        if path.is_dir() {
            copy(&path, &to.join(&name));
        } else {
            let contents = std::fs::read_to_string(&path)
                .expect("part file")
                .replace("{{name}}", "app")
                .replace("{{codeRoot}}", "src/")
                .replace("{{shapeRoot}}", "shape")
                .replace("{{specRoot}}", "..");
            std::fs::write(to.join(&name), contents).expect("copy");
        }
    }
}

/// Makes the project `piton init` starts for an application with Belay and
/// `adapters`, in a new directory: the application and belay parts, with the
/// belay part's shape and code files beneath the default roots and its
/// exports joined into the entry.
fn project(name: &str, adapters: &[(&str, &str)]) -> PathBuf {
    let parts = repo_root().join("plugins/init/templates");
    let dir = std::env::temp_dir().join(format!("piton-combination-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy(&parts.join("application"), &dir);
    let entry = std::fs::read_to_string(dir.join("spec/index.pi")).expect("entry");
    copy(&parts.join("belay/spec"), &dir.join("spec"));
    copy(&parts.join("belay/shape"), &dir.join("spec/shape"));
    copy(&parts.join("belay/code"), &dir.join("src"));
    let exports = std::fs::read_to_string(dir.join("spec/index.pi")).expect("exports");
    let (imports, rest) = entry.split_once("\n\n").expect("entry has imports");
    std::fs::write(dir.join("spec/index.pi"), format!("{imports}\n{exports}\n{rest}")).expect("entry");
    for (id, _) in adapters {
        let extra = parts.join("belay/.adapters").join(id);
        if extra.is_dir() {
            copy(&extra, &dir);
        }
    }

    let exports: Vec<&str> = adapters.iter().map(|(_, export)| *export).collect();
    let list: String = exports
        .iter()
        .map(|export| format!("        - {{{export}}}\n"))
        .collect();
    let config = format!(
        "use @piton/belay\nuse @piton/config\n\nfrom @piton/belay import {}\n\n\
         export piton-config Project:\n    root: ./spec\n    entry: ./spec/index.pi\n\n    plugins:\n        - {{Belay}}\n\n\
         belay-config Belay:\n    codeRoot: ./src\n    shapeRoot: ./spec/shape\n\n    adapters:\n{list}",
        exports.join(", ")
    );
    std::fs::write(dir.join("piton.config.pi"), config).expect("config");
    dir
}

/// Returns the plan for one combination in the form of the snapshot.
fn snapshot(adapters: &[(&str, &str)]) -> String {
    let name: Vec<&str> = adapters.iter().map(|(id, _)| *id).collect();
    let dir = project(&name.join("+"), adapters);
    let (project, _) = config::load(&dir, None);
    assert!(!project.plugins.is_empty(), "belay configured");
    let compilation = Compilation::build(project);
    let plan = belay::plan(&compilation);

    let mut out = String::new();
    for file in &plan.files {
        out.push_str(&format!(
            "==> {} ({}, {})\n{}\n",
            file.path.display(),
            file.target,
            file.kind.as_str(),
            shown(&file.path, &file.contents)
        ));
    }
    out.push_str("==> diagnostics\n");
    for diagnostic in &plan.diagnostics {
        out.push_str(&format!(
            "{} [{}] {}\n",
            diagnostic.severity, diagnostic.code, diagnostic.message
        ));
    }
    let _ = std::fs::remove_dir_all(&dir);
    out
}

/// Returns the contents of a file as the snapshot shows them. The snapshot
/// shows only the frontmatter of the `piton-fluency` skill, because its body
/// is the full fluency reference.
fn shown(path: &Path, contents: &str) -> String {
    let contents = contents.trim_end();
    if !path.to_string_lossy().contains("/piton-fluency/") {
        return contents.to_string();
    }
    match contents.match_indices("---\n").nth(1) {
        Some((end, _)) => format!("{}<the fluency reference>", &contents[..end + 4]),
        None => contents.to_string(),
    }
}

/// Returns each combination of one or more adapters, in the order of the list.
fn combinations() -> Vec<Vec<(&'static str, &'static str)>> {
    (1..1u32 << ADAPTERS.len())
        .map(|mask| {
            ADAPTERS
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(_, adapter)| *adapter)
                .collect()
        })
        .collect()
}

#[test]
fn every_combination_of_adapters_matches_its_snapshot() {
    let update = std::env::var_os("BELAY_UPDATE_SNAPSHOTS").is_some();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/belay_snapshots");
    let mut changed = Vec::new();
    for adapters in combinations() {
        let name = adapters.iter().map(|(id, _)| *id).collect::<Vec<_>>().join("+");
        let actual = snapshot(&adapters);
        assert!(
            !actual.contains("error ["),
            "{name} does not build:\n{}",
            actual.lines().filter(|line| line.starts_with("error")).collect::<Vec<_>>().join("\n")
        );
        let path = directory.join(format!("{name}.txt"));
        if update {
            std::fs::create_dir_all(&directory).expect("snapshots");
            std::fs::write(&path, &actual).expect("write snapshot");
            continue;
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_default();
        if expected != actual {
            changed.push(name);
        }
    }
    assert!(
        changed.is_empty(),
        "the output changed for {}; rerun with BELAY_UPDATE_SNAPSHOTS=1 and review the diff in tests/belay_snapshots",
        changed.join(", ")
    );
}
