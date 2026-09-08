//! Shared fixtures for the Belay tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use piton_belay::Belay;
use piton_core::builtin;
use piton_core::compile::compile;
use piton_core::db::Db;
use piton_core::framework::{Frameworks, OutputFile};
use piton_core::project::Project;

/// Write a throwaway project and build it.
pub fn build(files: &[(&str, &str)]) -> (PathBuf, Vec<OutputFile>, Vec<String>) {
    let root = unique_directory("piton-belay-test");
    for (path, contents) in files {
        let target = root.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, contents).unwrap();
    }

    let mut frameworks = Frameworks::new(vec![Box::new(Belay::new())]);
    let loaded = Project::load(&root, &frameworks);
    if let Some(configuration) = &loaded.compilation {
        frameworks.configure(&loaded.project, configuration, &loaded.project.framework_configs);
    }

    let mut db = Db::new();
    for module in builtin::modules().into_iter().chain(frameworks.modules()) {
        db.add_virtual_module(module.name, module.source);
    }
    db.set_root(&loaded.project.root);
    let entry = db.load(&loaded.project.entry).expect("entry loads");
    let compilation = compile(db, vec![entry], &frameworks);
    let messages: Vec<String> = compilation
        .diagnostics
        .iter()
        .filter(|it| it.is_error())
        .map(|it| it.message.clone())
        .collect();

    let mut outputs = Vec::new();
    for framework in &frameworks.active {
        outputs.extend(framework.emit(&compilation, &loaded.project).files);
    }
    (root, outputs, messages)
}

/// Resolve `..` without touching the filesystem, so a reference can be checked
/// against the files that will be written rather than the files that exist.
pub fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

pub fn find<'a>(outputs: &'a [OutputFile], root: &Path, suffix: &str) -> &'a str {
    outputs
        .iter()
        .find(|file| file.path.strip_prefix(root).is_ok_and(|it| it.to_string_lossy() == suffix))
        .map(|file| file.contents.as_str())
        .unwrap_or_else(|| {
            let listing: Vec<String> = outputs
                .iter()
                .map(|file| file.path.strip_prefix(root).unwrap_or(&file.path).display().to_string())
                .collect();
            panic!("no output at {suffix}; produced: {listing:#?}")
        })
}


/// A project configuration that turns on the Claude adapter.
pub const CONFIG: &str = "\
use @piton/config
use @piton/belay

from @piton/belay import ClaudeAdapter

export piton-config Config:
    root: ./spec
    entry: ./spec/index.pi

    frameworks:
        - {BelayConfiguration}

belay-config BelayConfiguration:
    codeRoot: ./src
    shapeRoot: ./spec/shape

    adapters:
        - {ClaudeAdapter}
";

/// A directory no other test can collide with.
///
/// Tests run in parallel threads of one process, so a timestamp alone is not
/// enough: two of them can start within the same nanosecond and then fight over
/// the same files.
fn unique_directory(prefix: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let ordinal = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("{prefix}-{}-{ordinal}", std::process::id()))
}
