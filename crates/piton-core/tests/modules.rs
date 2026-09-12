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
    let root: PathBuf = unique_directory("piton-modules");
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

/// Write a workspace, then compile the project in `project` with the named
/// directories declared as libraries.
///
/// Paths are written relative to the workspace, and the project's root is one
/// directory inside it, so a library can sit beside the project rather than
/// under it — which is the only arrangement that needs naming at all.
fn build_with_libraries(
    files: &[(&str, &str)],
    project: &str,
    libraries: &[(&str, &str)],
    entry: &str,
) -> Built {
    let workspace: PathBuf = unique_directory("piton-libraries");
    for (path, contents) in files {
        let target = workspace.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, contents).unwrap();
    }
    let mut db = Db::new();
    for module in builtin::modules() {
        db.add_virtual_module(module.name, module.source);
    }
    db.set_root(workspace.join(project));
    db.set_libraries(
        libraries.iter().map(|(name, path)| (name.to_string(), workspace.join(path))).collect(),
    );
    let entry = db.load(&workspace.join(entry)).expect("entry loads");
    let compilation = compile(db, vec![entry], &Frameworks::default());
    let errors = compilation
        .diagnostics
        .iter()
        .filter(|it| it.is_error())
        .map(|it| it.message.clone())
        .collect();
    Built { compilation, entry, errors }
}

#[test]
fn a_named_library_answers_a_rooted_import_from_outside_the_root() {
    let built = build_with_libraries(
        &[
            ("origin/main.pi", "from /customLib/Tool import Tool\n\na: {Tool.kind}\n"),
            ("lib/Tool.pi", "export anchor Tool:\n    kind: hammer\n"),
        ],
        "origin",
        &[("customLib", "lib")],
        "origin/main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("a"), serde_json::json!("hammer"));
}

#[test]
fn a_library_named_without_a_path_is_its_own_index() {
    let built = build_with_libraries(
        &[
            ("origin/main.pi", "from /customLib import Tool\n\na: {Tool.kind}\n"),
            ("lib/index.pi", "from ./Tool export *\n"),
            ("lib/Tool.pi", "export anchor Tool:\n    kind: anvil\n"),
        ],
        "origin",
        &[("customLib", "lib")],
        "origin/main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("a"), serde_json::json!("anvil"));
}

#[test]
fn the_root_answers_before_any_library_it_borrows() {
    // Declaring a library must never change what an import already resolved to,
    // so a name the project owns wins over the one it borrowed.
    let built = build_with_libraries(
        &[
            ("origin/main.pi", "from /lib/Tool import Tool\n\na: {Tool.kind}\n"),
            ("origin/lib/Tool.pi", "export anchor Tool:\n    kind: mine\n"),
            ("lib/Tool.pi", "export anchor Tool:\n    kind: borrowed\n"),
        ],
        "origin",
        &[("lib", "lib")],
        "origin/main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("a"), serde_json::json!("mine"));
}

#[test]
fn a_library_is_reached_only_by_the_name_it_was_given() {
    let built = build_with_libraries(
        &[
            ("origin/main.pi", "from /lib/Tool import Tool\n"),
            ("lib/Tool.pi", "export anchor Tool:\n    kind: hammer\n"),
        ],
        "origin",
        &[("customLib", "lib")],
        "origin/main.pi",
    );
    assert!(
        built.errors.iter().any(|it| it.contains("cannot find module `/lib/Tool`")),
        "{:?}",
        built.errors
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
    assert_eq!(built.value("a"), serde_json::json!(6));
}

#[test]
fn two_files_may_import_from_each_other() {
    // A circular import is legal. Each file sees what the other publishes, and
    // the prose references settle to the anchors' names.
    let built = build(
        &[
            (
                "main.pi",
                "from ./other import Other\n\nexport anchor Main:\n    mate: ${Other}\n\nboth: {Other.label}\n",
            ),
            (
                "other.pi",
                "from ./main import Main\n\nexport anchor Other:\n    label: other\n    mate: ${Main}\n",
            ),
        ],
        "main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("Main"), serde_json::json!({ "mate": "Other" }));
    assert_eq!(built.value("both"), serde_json::json!("other"));
}

#[test]
fn a_circular_import_still_reaches_through_a_module_index() {
    // The cycle runs through an `index.pi`, and the keyword it publishes has to
    // survive the round trip along with the base it declares.
    let built = build(
        &[
            ("main.pi", "from ./shapes import Child\n\nlabel: {Child.description}\n"),
            ("shapes/index.pi", "from ./Base export *\nfrom ./Child export *\n"),
            (
                "shapes/Base.pi",
                "from ./index import Extra\n\nexport anchor Base as base:\n    description: from base ${Extra}\n",
            ),
            (
                "shapes/Child.pi",
                "use ./index\n\nexport anchor Extra:\n    note: extra\n\nexport base Child:\n    description:\n        + {super.description}\n",
            ),
        ],
        "main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("label"), serde_json::json!(["from base Extra"]));
}

#[test]
fn a_name_that_no_file_exports_is_still_reported() {
    // Scopes settle before anything is reported, so the rounds that make a
    // circular import work do not hide a name that genuinely is not there.
    let built = build(
        &[
            ("main.pi", "from ./other import Missing\n"),
            ("other.pi", "from ./main import Nothing\n"),
        ],
        "main.pi",
    );
    let reported: Vec<&String> =
        built.errors.iter().filter(|it| it.contains("is not exported by")).collect();
    assert_eq!(reported.len(), 2, "{:?}", built.errors);
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
    // A width that is not the file's unit at all.
    let built = build(&[("main.pi", "a:\n    b: 1\nc:\n      d: 2\n")], "main.pi");
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

#[test]
fn a_block_may_be_inset_by_a_whole_number_of_indent_units() {
    // The specification's own anchor example is a two-space file whose last
    // string block is inset by four. Consistency is about the unit, not about
    // every block stepping exactly once: a body under a `- ` marker has to
    // clear the marker, and an over-indented string block is harmless because
    // its leading whitespace is discarded anyway.
    let built = build(&[("main.pi", "a:\n  b: 1\nc:\n    d: 2\n")], "main.pi");
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("c"), serde_json::json!({ "d": 2 }));
}

#[test]
fn a_broken_value_is_reported_where_the_value_is_written() {
    // The constraint comes from another file. The author who broke it is
    // reading this one, so this is where the problem has to be shown.
    let built = build(
        &[
            (
                "shapes.pi",
                "// Padding, so an offset from this file cannot be mistaken for one\n\
                 // in the file that actually has the problem.\n\
                 export abstract anchor Shape as shape:\n    description:: string\n",
            ),
            (
                "main.pi",
                "use ./shapes\n\nshape Broken:\n    description:\n        - a list\n        - not a string\n",
            ),
        ],
        "main.pi",
    );
    let reported: Vec<&piton_core::diag::Diagnostic> = built
        .compilation
        .diagnostics
        .iter()
        .filter(|it| it.message.contains("does not satisfy"))
        .collect();
    assert_eq!(reported.len(), 1, "{:?}", built.errors);

    let diagnostic = reported[0];
    let file = built.compilation.analysis.db.file(diagnostic.file);
    assert!(
        file.source.display().ends_with("main.pi"),
        "reported against {} instead of the file that broke it",
        file.source.display()
    );
    assert_eq!(&file.text[diagnostic.range], "description");
}

#[test]
fn reaching_is_transitive_and_stops_at_the_first_gap() {
    // The entry imports `a`, which imports `b`, which uses `c`. Everything on
    // that chain is reached. `orphan` imports `only-orphan-imports-me`, but
    // nothing imports `orphan`, so neither is reached: being imported is not
    // enough, it has to be imported by something that was itself reached.
    let built = build(
        &[
            ("main.pi", "from ./a import A

x: {A.v}
"),
            ("a.pi", "from ./b import B

export anchor A:
    v: {B.v}
"),
            ("b.pi", "use ./c

export anchor B:
    v: 1
"),
            ("c.pi", "export abstract anchor K as kw:
    v:: number
"),
            ("orphan.pi", "from ./only_orphan_imports_me import Hidden
"),
            ("only_orphan_imports_me.pi", "export anchor Hidden:
    v: 1
"),
        ],
        "main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);

    let name = |file: piton_core::FileId| {
        built
            .compilation
            .analysis
            .db
            .file(file)
            .source
            .as_path()
            .and_then(|path| path.file_name())
            .map(|it| it.to_string_lossy().to_string())
            .unwrap_or_default()
    };
    let reached: Vec<String> = built
        .compilation
        .reached_paths()
        .iter()
        .filter_map(|path| path.file_name())
        .map(|it| it.to_string_lossy().to_string())
        .collect();

    for expected in ["main.pi", "a.pi", "b.pi", "c.pi"] {
        assert!(reached.contains(&expected.to_string()), "{expected} is on the chain: {reached:?}");
    }
    for absent in ["orphan.pi", "only_orphan_imports_me.pi"] {
        assert!(!reached.contains(&absent.to_string()), "{absent} should not be reached");
    }

    // The chain names every hop, entry first.
    let target = built
        .compilation
        .analysis
        .db
        .files()
        .find(|file| name(file.id) == "c.pi")
        .expect("c.pi was loaded");
    let chain: Vec<String> =
        built.compilation.reach_chain(target.id).into_iter().map(name).collect();
    assert_eq!(chain, vec!["main.pi", "a.pi", "b.pi", "c.pi"], "`use` is a hop like any other");
}

#[test]
fn a_chain_takes_the_shortest_route_when_there_are_several() {
    let built = build(
        &[
            ("main.pi", "from ./shared import S
from ./long import L

x: {S.v}
y: {L.v}
"),
            ("shared.pi", "export anchor S:
    v: 1
"),
            ("long.pi", "from ./shared import S

export anchor L:
    v: {S.v}
"),
        ],
        "main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    let name = |file: piton_core::FileId| {
        built.compilation.analysis.db.file(file).source.as_path()
            .and_then(|p| p.file_name()).map(|it| it.to_string_lossy().to_string()).unwrap_or_default()
    };
    let shared = built
        .compilation
        .analysis
        .db
        .files()
        .find(|file| name(file.id) == "shared.pi")
        .expect("shared.pi was loaded");
    let chain: Vec<String> =
        built.compilation.reach_chain(shared.id).into_iter().map(name).collect();
    assert_eq!(chain, vec!["main.pi", "shared.pi"], "the direct route wins over the long one");
}

#[test]
fn only_files_reachable_from_the_entry_are_compiled() {
    let built = build(
        &[
            ("main.pi", "from ./used import Thing\n\nx: {Thing.value}\n"),
            ("used.pi", "export anchor Thing:\n    value: 1\n"),
            // Never imported by anything, and full of problems nobody will see.
            ("orphan.pi", "broken: {nonexistent}\nalso: {missing.thing}\n"),
        ],
        "main.pi",
    );
    assert!(built.errors.is_empty(), "the orphan's problems are not reported: {:?}", built.errors);

    let reached: Vec<String> = built
        .compilation
        .reached_paths()
        .iter()
        .filter_map(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .collect();
    assert!(reached.contains(&"main.pi".to_string()));
    assert!(reached.contains(&"used.pi".to_string()));
    assert!(!reached.contains(&"orphan.pi".to_string()), "{reached:?}");

    let root = built.compilation.analysis.db.root().expect("a root").to_path_buf();
    assert!(built.compilation.reaches(&root.join("used.pi")));
    assert!(!built.compilation.reaches(&root.join("orphan.pi")));
}

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

#[test]
fn a_reach_chain_names_the_import_that_made_each_hop() {
    let built = build(
        &[
            ("main.pi", "from ./a import A\n\nx: {A.v}\n"),
            ("a.pi", "// a comment first\nfrom ./nested/b import B\n\nexport anchor A:\n    v: {B.v}\n"),
            ("nested/b.pi", "export anchor B:\n    v: 1\n"),
        ],
        "main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    let name = |file: piton_core::FileId| {
        built.compilation.analysis.db.file(file).source.as_path()
            .and_then(|p| p.file_name()).map(|it| it.to_string_lossy().to_string()).unwrap_or_default()
    };
    let target = built
        .compilation
        .analysis
        .db
        .files()
        .find(|file| name(file.id) == "b.pi")
        .expect("b.pi was loaded");

    let steps = built.compilation.reach_steps(target.id);
    let described: Vec<(String, String, String)> = steps
        .iter()
        .map(|step| (name(step.from), step.specifier.clone(), name(step.to)))
        .collect();
    assert_eq!(
        described,
        vec![
            ("main.pi".to_string(), "./a".to_string(), "a.pi".to_string()),
            ("a.pi".to_string(), "./nested/b".to_string(), "b.pi".to_string()),
        ]
    );

    // The range has to point at the specifier, so a line number can be shown.
    let hop = &steps[1];
    let text = &built.compilation.analysis.db.file(hop.from).text;
    assert_eq!(&text[hop.range], "./nested/b");
    assert_eq!(text[..usize::from(hop.range.start())].matches('\n').count(), 1, "on line 2");

    // An entry point is reached by nothing.
    let entry = built.compilation.entries[0];
    assert!(built.compilation.reach_steps(entry).is_empty());

    // And the reverse question: who imports this?
    let importers = built.compilation.importers(target.id);
    assert_eq!(importers.len(), 1);
    assert_eq!(name(importers[0].from), "a.pi");
    assert_eq!(importers[0].specifier, "./nested/b");
}

/// Write a throwaway workspace with a shared root and compile `entry`.
fn build_with_shared_root(
    files: &[(&str, &str)],
    project: &str,
    shared: Option<&str>,
    entry: &str,
) -> Built {
    let workspace: PathBuf = unique_directory("piton-shared-root");
    for (path, contents) in files {
        let target = workspace.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, contents).unwrap();
    }
    let mut db = Db::new();
    for module in builtin::modules() {
        db.add_virtual_module(module.name, module.source);
    }
    db.set_root(workspace.join(project));
    db.set_shared_root(shared.map(|path| workspace.join(path)));
    let entry = db.load(&workspace.join(entry)).expect("entry loads");
    let compilation = compile(db, vec![entry], &Frameworks::default());
    let errors = compilation
        .diagnostics
        .iter()
        .filter(|it| it.is_error())
        .map(|it| it.message.clone())
        .collect();
    Built { compilation, entry, errors }
}

#[test]
fn a_shared_import_resolves_against_the_shared_root() {
    let built = build_with_shared_root(
        &[
            ("origin/main.pi", "from //Tool import Tool\n\na: {Tool.kind}\n"),
            ("shared/Tool.pi", "export anchor Tool:\n    kind: hammer\n"),
        ],
        "origin",
        Some("shared"),
        "origin/main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("a"), serde_json::json!("hammer"));
}

#[test]
fn a_shared_import_reaches_a_directory_index() {
    let built = build_with_shared_root(
        &[
            ("origin/main.pi", "from //tools import Tool\n\na: {Tool.kind}\n"),
            ("shared/tools/index.pi", "from ./Tool export *\n"),
            ("shared/tools/Tool.pi", "export anchor Tool:\n    kind: anvil\n"),
        ],
        "origin",
        Some("shared"),
        "origin/main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("a"), serde_json::json!("anvil"));
}

#[test]
fn two_projects_share_one_root() {
    let files = &[
        ("origin/main.pi", "from //Tool import Tool\n\na: {Tool.kind}\n"),
        ("substrate/main.pi", "from //Tool import Tool\n\na: {Tool.kind}\n"),
        ("shared/Tool.pi", "export anchor Tool:\n    kind: hammer\n"),
    ];
    for project in ["origin", "substrate"] {
        let built = build_with_shared_root(
            files,
            project,
            Some("shared"),
            &format!("{project}/main.pi"),
        );
        assert!(built.errors.is_empty(), "{project}: {:?}", built.errors);
        assert_eq!(built.value("a"), serde_json::json!("hammer"));
    }
}

#[test]
fn a_shared_import_without_a_shared_root_says_so() {
    let built = build_with_shared_root(
        &[
            ("origin/main.pi", "from //Tool import Tool\n\na: {Tool.kind}\n"),
            ("shared/Tool.pi", "export anchor Tool:\n    kind: hammer\n"),
        ],
        "origin",
        None,
        "origin/main.pi",
    );
    assert!(
        built.errors.iter().any(|it| it.contains("sharedRoot")),
        "expected the error to name the setting that is missing: {:?}",
        built.errors
    );
}

#[test]
fn a_shared_import_does_not_fall_back_to_the_project_root() {
    // `//Tool` is not `/Tool`: a file the root happens to hold must not answer
    // a shared import, or moving the shared directory would silently start
    // resolving somewhere else.
    let built = build_with_shared_root(
        &[
            ("origin/main.pi", "from //Tool import Tool\n\na: {Tool.kind}\n"),
            ("origin/Tool.pi", "export anchor Tool:\n    kind: rooted\n"),
            ("shared/Other.pi", "export anchor Other:\n    kind: shared\n"),
        ],
        "origin",
        Some("shared"),
        "origin/main.pi",
    );
    assert!(
        built.errors.iter().any(|it| it.contains("cannot find module `//Tool`")),
        "{:?}",
        built.errors
    );
}

#[test]
fn a_rooted_import_is_not_a_shared_one() {
    let built = build_with_shared_root(
        &[
            ("origin/main.pi", "from /Tool import Tool\n\na: {Tool.kind}\n"),
            ("origin/Tool.pi", "export anchor Tool:\n    kind: rooted\n"),
            ("shared/Tool.pi", "export anchor Tool:\n    kind: shared\n"),
        ],
        "origin",
        Some("shared"),
        "origin/main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
    assert_eq!(built.value("a"), serde_json::json!("rooted"));
}

#[test]
fn use_takes_a_shared_specifier() {
    let built = build_with_shared_root(
        &[
            ("origin/main.pi", "use //Keywords\n\ntool Hammer:\n    kind: hammer\n"),
            (
                "shared/Keywords.pi",
                "export anchor Tool as tool:\n    kind:: string\n",
            ),
        ],
        "origin",
        Some("shared"),
        "origin/main.pi",
    );
    assert!(built.errors.is_empty(), "{:?}", built.errors);
}
