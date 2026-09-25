//! Snapshots what Belay writes for every combination of the built-in adapters.
//!
//! The project is the `belay/application` template `piton init` writes, which
//! has an instruction, a skill, a command, an agent, and references, so each
//! combination exercises what the tools share: one reference tree, one
//! AGENTS.md, and the skills one tool finds in another's directory.
//!
//! Each combination's plan -- every file with its contents, then every
//! diagnostic -- is compared with `tests/snapshots/<combination>.txt`. Run with
//! `BELAY_UPDATE_SNAPSHOTS=1` to rewrite them after an intended change, and
//! read the diff.

use std::path::{Path, PathBuf};

use piton_compile::{config, Compilation, Framework};

const ADAPTERS: &[(&str, &str)] = &[
    ("claude-code", "ClaudeCodeAdapter"),
    ("codex", "CodexAdapter"),
    ("opencode", "OpenCodeAdapter"),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

/// Copies `from` into `to`, skipping the template's description.
fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("directory");
    for entry in std::fs::read_dir(from).expect("template").flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name == ".template" || name == ".adapters" {
            continue;
        }
        if path.is_dir() {
            copy(&path, &to.join(&name));
        } else {
            std::fs::copy(&path, to.join(&name)).expect("copy");
        }
    }
}

/// The template's project, configured for `adapters`, in a fresh directory.
fn project(name: &str, adapters: &[(&str, &str)]) -> PathBuf {
    let template = repo_root().join("create-templates/belay/application");
    let dir = std::env::temp_dir().join(format!("piton-combination-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy(&template, &dir);
    for (id, _) in adapters {
        let extra = template.join(".adapters").join(id);
        if extra.is_dir() {
            copy(&extra, &dir);
        }
    }

    let config = std::fs::read_to_string(dir.join("piton.config.pi")).expect("config");
    let exports: Vec<&str> = adapters.iter().map(|(_, export)| *export).collect();
    let list: String = exports
        .iter()
        .map(|export| format!("        - {{{export}}}\n"))
        .collect();
    let config = config
        .replace(
            "from @piton/belay import ClaudeCodeAdapter",
            &format!("from @piton/belay import {}", exports.join(", ")),
        )
        .replace("        - {ClaudeCodeAdapter}\n", &list);
    std::fs::write(dir.join("piton.config.pi"), config).expect("config");
    dir
}

/// The plan for one combination, as the snapshot records it.
fn snapshot(adapters: &[(&str, &str)]) -> String {
    let name: Vec<&str> = adapters.iter().map(|(id, _)| *id).collect();
    let dir = project(&name.join("+"), adapters);
    let (project, _) = config::load(&dir, None);
    let belay = project
        .frameworks
        .iter()
        .find_map(|framework| match framework {
            Framework::Belay(config) => Some(config.clone()),
        })
        .expect("belay configured");
    let compilation = Compilation::build(project);
    let plan = piton_belay::plan(&compilation, &belay);

    let mut out = String::new();
    for file in &plan.files {
        out.push_str(&format!(
            "==> {} ({}, {})\n{}\n",
            file.path.display(),
            file.target,
            file.kind.as_str(),
            file.contents.trim_end()
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

/// Every non-empty combination of the adapters, in their listed order.
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
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
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
        "the output changed for {}; rerun with BELAY_UPDATE_SNAPSHOTS=1 and review the diff in tests/snapshots",
        changed.join(", ")
    );
}
