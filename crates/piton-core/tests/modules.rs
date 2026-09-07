//! Imports, exports, modules, and `use`, exercised against real files.

use std::path::PathBuf;

use piton_core::compile::{compile, Compilation};
use piton_core::db::Db;
use piton_core::framework::Frameworks;
use piton_core::serialize::to_json;
use piton_core::{builtin, FileId};

struct Built {
    compilation: Compilation,
    entry: FileId,
    errors: Vec<String>,
}

/// Write a throwaway project rooted at `root` and compile `entry`.
fn build(files: &[(&str, &str)], entry: &str) -> Built {
    let root: PathBuf = std::env::temp_dir().join(format!(
        "piton-modules-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    for (path, contents) in files {
        let target = root.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, contents).unwrap();
    }
    let mut db = Db::new();
    for module in builtin::modules() {
        db.add_virtual_module(module.name, module.source);
    }
    db.set_root(&root);
    let entry = db.load(&root.join(entry)).expect("entry loads");
    let compilation = compile(db, vec![entry], &Frameworks::default());
    let errors = compilation
        .diagnostics
        .iter()
        .filter(|it| it.is_error())
        .map(|it| it.message.clone())
        .collect();
    Built { compilation, entry, errors }
}

impl Built {
    fn value(&self, name: &str) -> serde_json::Value {
        self.compilation
            .file_values(self.entry)
            .into_iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| to_json(&value))
            .unwrap_or(serde_json::Value::Null)
    }
}

#[test]
fn a_directory_with_an_index_is_a_module() {
    let built = build(
        &[
            ("main.pi", "from ./shapes import Button\n\nname: {Button.label}\n"),
            ("shapes/index.pi", "from ./Button export *\n"),
            ("shapes/Button.pi", "export anchor Button:\n    label: Press me\n"),
        ],
        "main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("name"), serde_json::json!("Press me"));
}

#[test]
fn imports_may_be_aliased_and_the_original_name_stays_hidden() {
    let built = build(
        &[
            (
                "main.pi",
                "from ./values import pi SliceOf\n\ngood: {SliceOf}\nbad: {pi}\n",
            ),
            ("values.pi", "export pi: 3.5\n"),
        ],
        "main.pi",
    );
    assert_eq!(built.value("good"), serde_json::json!(3.5));
    assert!(
        built.errors.iter().any(|it| it.contains("cannot find `pi`")),
        "{:?}",
        built.errors
    );
}

#[test]
fn only_exported_names_can_be_imported() {
    let built = build(
        &[
            ("main.pi", "from ./values import hidden\n"),
            ("values.pi", "hidden: 1\nexport shown: 2\n"),
        ],
        "main.pi",
    );
    assert!(
        built.errors.iter().any(|it| it.contains("`hidden` is not exported")),
        "{:?}",
        built.errors
    );
}

#[test]
fn from_export_re_exports_with_and_without_aliases() {
    let built = build(
        &[
            ("main.pi", "from ./mod import Renamed, Direct\n\na: {Renamed.x}\nb: {Direct.x}\n"),
            (
                "mod/index.pi",
                "from ./One export One Renamed\nfrom ./Two export *\n",
            ),
            ("mod/One.pi", "export anchor One:\n    x: one\n"),
            ("mod/Two.pi", "export anchor Direct:\n    x: two\n"),
        ],
        "main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("a"), serde_json::json!("one"));
    assert_eq!(built.value("b"), serde_json::json!("two"));
}

#[test]
fn export_republishes_an_imported_name() {
    let built = build(
        &[
            ("main.pi", "from ./mid import Thing\n\na: {Thing.x}\n"),
            ("mid.pi", "from ./leaf import Thing\nexport Thing\n"),
            ("leaf.pi", "export anchor Thing:\n    x: leaf\n"),
        ],
        "main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("a"), serde_json::json!("leaf"));
}

#[test]
fn use_brings_in_keywords_and_import_does_not() {
    let built = build(
        &[
            (
                "main.pi",
                "use ./keywords\n\nmy-shape Concrete:\n    description: made with a keyword\n",
            ),
            (
                "keywords.pi",
                "export abstract anchor MyShape as my-shape:\n    description:: string\n",
            ),
        ],
        "main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(
        built.value("Concrete"),
        serde_json::json!({ "description": "made with a keyword" })
    );

    let without_use = build(
        &[
            (
                "main.pi",
                "from ./keywords import MyShape\n\nmy-shape Concrete:\n    description: nope\n",
            ),
            (
                "keywords.pi",
                "export abstract anchor MyShape as my-shape:\n    description:: string\n",
            ),
        ],
        "main.pi",
    );
    assert!(
        without_use.errors.iter().any(|it| it.contains("is not a keyword here")),
        "{:?}",
        without_use.errors
    );
}

#[test]
fn absolute_imports_resolve_from_the_project_root() {
    let built = build(
        &[
            ("deep/nested/main.pi", "from /shapes/Button import Button\n\na: {Button.label}\n"),
            ("shapes/Button.pi", "export anchor Button:\n    label: Press\n"),
        ],
        "deep/nested/main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("a"), serde_json::json!("Press"));
}

#[test]
fn imports_may_span_lines() {
    let built = build(
        &[
            (
                "main.pi",
                "from ./values import\n    alpha,\n    bravo,\n    charlie\n\na: {alpha + bravo + charlie}\n",
            ),
            ("values.pi", "export alpha: 1\nexport bravo: 2\nexport charlie: 3\n"),
        ],
        "main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("a"), serde_json::json!(6.0));
}

#[test]
fn an_unresolvable_module_is_reported_once() {
    let built = build(&[("main.pi", "from ./nowhere import Thing\n")], "main.pi");
    assert!(
        built.errors.iter().any(|it| it.contains("cannot find module `./nowhere`")),
        "{:?}",
        built.errors
    );
}

#[test]
fn numbers_keep_the_spelling_they_were_written_with() {
    let built = build(
        &[("main.pi", "version: 1.0\nmessage: Piton is at version ${version}\n")],
        "main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("message"), serde_json::json!("Piton is at version 1.0"));
}

#[test]
fn indentation_must_be_consistent_within_a_file() {
    let built = build(
        &[("main.pi", "a:\n  b: 1\nc:\n    d: 2\n")],
        "main.pi",
    );
    assert!(
        built.errors.iter().any(|it| it.contains("inconsistent indentation")),
        "{:?}",
        built.errors
    );

    let tabs = build(&[("main.pi", "a:\n \tb: 1\n")], "main.pi");
    assert!(
        tabs.errors.iter().any(|it| it.contains("mixes tabs and spaces")),
        "{:?}",
        tabs.errors
    );
}
