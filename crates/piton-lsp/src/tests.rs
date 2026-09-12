//! Feature-level tests for the language server, driven through a real
//! workspace on disk so that resolution and evaluation are exercised too.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use piton_core::framework::Frameworks;
use piton_core::FileId;
use tower_lsp::lsp_types::{Position, Url};

use crate::world::{View, Workspace};
use crate::{actions, completion, hover, navigation, tokens};

/// Write a throwaway project and analyse it.
fn workspace(files: &[(&str, &str)]) -> (PathBuf, Workspace) {
    let root = unique_directory("piton-lsp-test");
    for (path, contents) in files {
        let target = root.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, contents).unwrap();
    }
    let mut workspace = Workspace::new(Frameworks::default as fn() -> Frameworks);
    workspace.set_roots(vec![root.clone()]);
    (root, workspace)
}

/// Analyse the workspace, and take the project that owns `path`.
///
/// Which project answers for a file is part of what these tests are checking,
/// so every one of them asks for a view by naming a file rather than assuming
/// the workspace holds a single project.
fn view_of(workspace: &mut Workspace, path: impl AsRef<Path>) -> Arc<View> {
    let path = path.as_ref();
    let snapshot = workspace.snapshot();
    let (view, _) = snapshot
        .locate(path)
        .unwrap_or_else(|| panic!("no project analysed {}", path.display()));
    view.clone()
}

/// The byte offset of the first occurrence of `needle` in a file.
fn offset_of(text: &str, needle: &str) -> piton_syntax::TextSize {
    piton_syntax::TextSize::new(text.find(needle).expect(needle) as u32)
}

const SHAPES: &str = "\
// Shapes shared across the project.
abstract anchor Shape as shape:
    description:: string

export anchor Base:
    name: Base
    summary: This is ${this.name}
";

const MAIN: &str = "\
from ./shapes import Base

anchor Child extends Base:
    name: Child
    detail: This is ${self.name}

pi: 3.14
";

#[test]
fn diagnostics_come_from_the_compiler() {
    let (root, mut workspace) = workspace(&[
        ("shapes.pi", SHAPES),
        ("main.pi", "from ./shapes import Missing\n\nbroken: {nope}\n"),
    ]);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let file = view.file_for(&root.join("main.pi")).expect("main is analysed");
    let messages: Vec<&str> = view
        .compilation
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.file == file)
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();
    assert!(messages.iter().any(|it| it.contains("is not exported")), "{messages:?}");
    assert!(messages.iter().any(|it| it.contains("cannot find `nope`")), "{messages:?}");
}

const SCOPED_PROJECT: &[(&str, &str)] = &[
    ("piton.config.pi", "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n"),
    ("spec/index.pi", "from ./Real export *\n"),
    ("spec/Real.pi", "export anchor Real:\n    x: 1\n"),
    // Generated output, and a fixture that is deliberately not a valid program.
    ("editors/fixture/highlight.pi", "unknown-keyword Broken:\n    x: {missing}\n"),
];

#[test]
fn a_project_root_says_what_belongs_to_the_project() {
    let (root, mut workspace) = workspace(SCOPED_PROJECT);
    let view = view_of(&mut workspace, root.join("spec/Real.pi"));

    // The declared root is analysed.
    assert!(view.file_for(&root.join("spec/Real.pi")).is_some());
    // The configuration is too, even though it sits outside that root.
    assert!(view.file_for(&root.join("piton.config.pi")).is_some());
    // Generated output outside the root is not, so its problems stay quiet.
    assert!(
        view.file_for(&root.join("editors/fixture/highlight.pi")).is_none(),
        "a file outside the project root is not a project source"
    );
    assert!(
        !view.compilation.diagnostics.iter().any(|it| it.message.contains("unknown-keyword")),
        "nothing outside the root should be reported"
    );
}

#[test]
fn a_file_that_is_open_is_analysed_wherever_it_lives() {
    let (root, mut workspace) = workspace(SCOPED_PROJECT);
    let outside = root.join("editors/fixture/highlight.pi");
    workspace.open(outside.clone(), std::fs::read_to_string(&outside).unwrap());
    let view = view_of(&mut workspace, &outside);

    let file = view.file_for(&outside).expect("an open file is always analysed");
    let messages: Vec<&str> = view
        .compilation
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.file == file)
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();
    assert!(messages.iter().any(|it| it.contains("is not a keyword here")), "{messages:?}");
}

// ---- several projects in one workspace ------------------------------------

/// Two projects side by side under one workspace directory, each with a root of
/// its own and each reaching for it through a rooted import.
const SIBLINGS: &[(&str, &str)] = &[
    (
        "origin/piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./src\n",
    ),
    ("origin/src/lib/Tool.pi", "export anchor Tool:\n    kind: hammer\n"),
    ("origin/src/index.pi", "from /lib/Tool import Tool\n\nanchor Use extends Tool:\n    n: 1\n"),
    (
        "substrate/piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./source\n",
    ),
    ("substrate/source/lib/Other.pi", "export anchor Other:\n    kind: anvil\n"),
    (
        "substrate/source/index.pi",
        "from /lib/Other import Other\n\nanchor Use extends Other:\n    n: 2\n",
    ),
];

#[test]
fn sibling_projects_each_resolve_against_their_own_root() {
    let (root, mut workspace) = workspace(SIBLINGS);

    for (project, module) in [("origin", "src"), ("substrate", "source")] {
        let entry = root.join(project).join(module).join("index.pi");
        let view = view_of(&mut workspace, &entry);
        assert_eq!(
            view.project.root,
            root.join(project).join(module),
            "{project} should be analysed against its own root"
        );
        let messages: Vec<&str> =
            view.compilation.diagnostics.iter().map(|it| it.message.as_str()).collect();
        assert!(messages.is_empty(), "{project} should be clean: {messages:?}");
    }
}

#[test]
fn a_sibling_project_is_not_in_scope() {
    // `substrate` reaches for a module only `origin` has. A workspace-wide
    // compilation would have found it; two projects must not.
    let mut files = SIBLINGS.to_vec();
    files.push((
        "substrate/source/borrow.pi",
        "from /lib/Tool import Tool\n\nanchor Borrowed extends Tool:\n    n: 3\n",
    ));
    let (root, mut workspace) = workspace(&files);

    let borrow = root.join("substrate/source/borrow.pi");
    let view = view_of(&mut workspace, &borrow);
    let file = view.file_for(&borrow).unwrap();
    let messages: Vec<&str> = view
        .compilation
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.file == file)
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();
    assert!(
        messages.iter().any(|it| it.contains("/lib/Tool")),
        "the sibling's module should not resolve: {messages:?}"
    );
    // And the file it reached for is not in this project's database at all.
    assert!(view.file_for(&root.join("origin/src/lib/Tool.pi")).is_none());
}

/// Two sibling projects and a library beside them, which is the shape a
/// workspace takes once code is shared: `origin` borrows the workspace-level
/// `lib`, `substrate` has a `lib` of its own, and `/lib` means a different file
/// in each.
const SHARED_LIBRARY: &[(&str, &str)] = &[
    ("lib/Tool.pi", "export abstract anchor Tool as tool:\n    purpose:: string\n"),
    ("lib/index.pi", "from ./Tool export *\n"),
    (
        "origin/piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n\n    \
         libraries:\n        customLib: ../lib\n",
    ),
    ("origin/spec/index.pi", "from /parts export *\n"),
    ("origin/spec/parts/index.pi", "from ./Hammer export *\n"),
    (
        "origin/spec/parts/Hammer.pi",
        "use /customLib\n\nexport tool Hammer:\n    purpose: drive nails\n",
    ),
    (
        "substrate/piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./\n",
    ),
    ("substrate/lib/index.pi", "export abstract anchor Part as part:\n    size:: number\n"),
    ("substrate/index.pi", "use /lib\n\npart Bolt:\n    size: 4\n"),
];

#[test]
fn a_named_library_is_reachable_from_the_project_that_declared_it() {
    let (root, mut workspace) = workspace(SHARED_LIBRARY);
    let hammer = root.join("origin/spec/parts/Hammer.pi");
    let view = view_of(&mut workspace, &hammer);

    let messages: Vec<&str> =
        view.compilation.diagnostics.iter().map(|it| it.message.as_str()).collect();
    assert!(messages.is_empty(), "the library should resolve: {messages:?}");
    // The library's file is analysed by the project that borrowed it, so
    // navigation reaches across the boundary.
    assert!(view.file_for(&root.join("lib/Tool.pi")).is_some(), "the library is in scope");
}

#[test]
fn a_library_belongs_only_to_the_project_that_named_it() {
    // `substrate` declares no library, so its `/lib` is its own and the
    // workspace-level one is not in its database at all.
    let (root, mut workspace) = workspace(SHARED_LIBRARY);
    let view = view_of(&mut workspace, root.join("substrate/index.pi"));

    let messages: Vec<&str> =
        view.compilation.diagnostics.iter().map(|it| it.message.as_str()).collect();
    assert!(messages.is_empty(), "substrate resolves its own lib: {messages:?}");
    assert!(view.file_for(&root.join("substrate/lib/index.pi")).is_some());
    assert!(
        view.file_for(&root.join("lib/Tool.pi")).is_none(),
        "a library nobody named is not in scope"
    );
}

#[test]
fn a_library_that_is_not_there_is_reported_on_the_config() {
    let (root, mut workspace) = workspace(&[
        (
            "piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n\n    \
             libraries:\n        customLib: ../lib\n",
        ),
        ("spec/main.pi", "from /customLib/Tool import Tool\n"),
    ]);
    let config = root.join("piton.config.pi");
    let view = view_of(&mut workspace, &config);

    let diagnostics: Vec<_> = view.compilation.diagnostics.iter().collect();
    assert_eq!(diagnostics.len(), 1, "one cause, one diagnostic: {diagnostics:?}");
    assert_eq!(view.file_for(&config), Some(diagnostics[0].file));
    assert!(diagnostics[0].message.contains("library `customLib` is `../lib`"), "{diagnostics:?}");
}

#[test]
fn a_directory_that_declares_nothing_is_not_a_project_root() {
    // An editor opens a directory; that does not make it a project. Taking it
    // for one would make `/lib/Tool` mean whatever happened to be beside the
    // file, which is a different answer for every window the file is opened in.
    let (root, mut workspace) = workspace(&[
        ("lib/Tool.pi", "export anchor Tool:\n    kind: hammer\n"),
        ("loose.pi", "from /lib/Tool import Tool\n"),
    ]);
    let view = view_of(&mut workspace, root.join("loose.pi"));
    let file = view.file_for(&root.join("loose.pi")).unwrap();

    let messages: Vec<&str> = view
        .compilation
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.file == file)
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();
    assert!(
        messages.iter().any(|it| it.contains("needs a project root")),
        "the import should say what is missing: {messages:?}"
    );
}

#[test]
fn a_project_whose_configuration_is_broken_reports_only_the_configuration() {
    // `entry` names a file that is not there, so the compiler refuses to build
    // this project at all. Its sources are written against a root the author
    // has not finished declaring, and reporting what they fail to import would
    // bury the one line that can be fixed.
    let (root, mut workspace) = workspace(&[
        (
            "piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./\n    \
             entry: ./index.pi\n",
        ),
        ("spec/Broken.pi", "use /lib\n\nwidget Thing:\n    x: {nope}\n"),
    ]);
    let config = root.join("piton.config.pi");
    let view = view_of(&mut workspace, &config);

    let diagnostics: Vec<_> = view.compilation.diagnostics.iter().collect();
    assert_eq!(diagnostics.len(), 1, "only the configuration: {diagnostics:?}");
    assert!(diagnostics[0].message.contains("`entry` is `./index.pi`"), "{diagnostics:?}");
    // The sources are neither analysed here nor picked up by the bare project,
    // which would report them against the workspace root instead.
    let source = root.join("spec/Broken.pi");
    assert!(view.file_for(&source).is_none(), "a broken project analyses no sources");
    assert!(
        workspace.snapshot().locate(&source).is_none(),
        "and nothing else claims them either"
    );
}

#[test]
fn an_open_config_file_stays_with_the_project_it_declares() {
    // The configuration sits outside the root it declares, so nothing but an
    // explicit claim keeps it out of the fallback project.
    let (root, mut workspace) = workspace(SCOPED_PROJECT);
    let config = root.join("piton.config.pi");
    workspace.open(config.clone(), std::fs::read_to_string(&config).unwrap());

    let view = view_of(&mut workspace, &config);
    assert_eq!(view.project.config_path.as_deref(), Some(config.as_path()));
    assert_eq!(view.project.root, root.join("spec"));
}

#[test]
fn a_root_that_is_not_there_is_reported_on_the_config() {
    let (root, mut workspace) = workspace(&[
        (
            "piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n",
        ),
        // Written against a root that does not exist, so nothing here resolves.
        ("src/index.pi", "from /lib/Tool import Tool\n"),
    ]);
    let config = root.join("piton.config.pi");
    let view = view_of(&mut workspace, &config);

    let diagnostics: Vec<_> = view.compilation.diagnostics.iter().collect();
    assert_eq!(diagnostics.len(), 1, "one cause, one diagnostic: {diagnostics:?}");
    let only = diagnostics[0];
    assert_eq!(view.file_for(&config), Some(only.file), "reported on the config file");
    assert!(only.message.contains("`root` is `./spec`"), "{}", only.message);
    assert!(only.message.contains("/lib/Tool"), "{}", only.message);
    // The line that set `root`, not the top of the file.
    let text = view.text(only.file);
    assert!(text[only.range].starts_with("root:"), "{:?}", &text[only.range]);
}

#[test]
fn definition_and_references_cross_files() {
    let (root, mut workspace) = workspace(&[("shapes.pi", SHAPES), ("main.pi", MAIN)]);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let main = view.file_for(&root.join("main.pi")).unwrap();
    let shapes = view.file_for(&root.join("shapes.pi")).unwrap();

    let at = offset_of(MAIN, "Base\n\nanchor") ;
    let resolved = navigation::resolve(&view, main, at).expect("resolves `Base`");
    let (file, range) = navigation::definition(&view, &resolved.target).expect("has a home");
    assert_eq!(file, shapes);
    assert_eq!(&view.text(shapes)[range], "Base");

    let references = navigation::references(&view, &resolved.target);
    assert!(references.len() >= 2, "expected the import and the extends: {references:?}");
    assert!(references.iter().any(|(file, _)| *file == main));
}

#[test]
fn implementations_list_concrete_anchors() {
    let source = "\
abstract anchor Shape as shape:
    description:: string

shape One:
    description: first

shape Two:
    description: second
";
    let (root, mut workspace) = workspace(&[("main.pi", source)]);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let file = view.file_for(&root.join("main.pi")).unwrap();
    let at = offset_of(source, "Shape as");
    let resolved = navigation::resolve(&view, file, at).unwrap();
    assert_eq!(navigation::implementations(&view, &resolved.target).len(), 2);
}

#[test]
fn hover_explains_anchors_properties_and_types() {
    let (root, mut workspace) = workspace(&[("shapes.pi", SHAPES), ("main.pi", MAIN)]);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let main = view.file_for(&root.join("main.pi")).unwrap();

    let anchor = navigation::resolve(&view, main, offset_of(MAIN, "Child")).unwrap();
    let text = hover::hover(&view, &anchor).unwrap();
    assert!(text.contains("anchor Child extends Base"), "{text}");
    assert!(text.contains("summary"), "{text}");

    let variable = navigation::resolve(&view, main, offset_of(MAIN, "pi:")).unwrap();
    let text = hover::hover(&view, &variable).unwrap();
    assert!(text.contains("3.14"), "{text}");
}

/// The labels completion offers at the position just after `marker`.
fn labels_after(files: &[(&str, &str)], file: &str, marker: &str) -> Vec<String> {
    let (root, mut workspace) = workspace(files);
    let view = view_of(&mut workspace, root.join(file));
    let id = view.file_for(&root.join(file)).expect("file is analysed");
    let text = view.text(id).to_string();
    let at = text.find(marker).unwrap_or_else(|| panic!("{marker:?} not in:\n{text}"))
        + marker.len();
    completion::complete(&view, id, piton_syntax::TextSize::new(at as u32))
        .into_iter()
        .map(|item| item.label)
        .collect()
}

const KEYWORD_SOURCE: &str = "\
abstract anchor Shape as shape:
    description:: string
    weight:: number

shape Concrete:
    description: a description
";

#[test]
fn a_top_level_line_offers_only_what_can_start_a_declaration() {
    let labels = labels_after(&[("main.pi", KEYWORD_SOURCE)], "main.pi", "\n\n");
    assert!(labels.iter().any(|it| it == "anchor"), "{labels:?}");
    assert!(labels.iter().any(|it| it == "shape"), "{labels:?}");
    assert!(labels.iter().any(|it| it == "use"), "{labels:?}");
    // These are only legal in the middle of a declaration, never at its start.
    for wrong in ["extends", "as", "import", "this", "self", "super", "true", "null"] {
        assert!(!labels.iter().any(|it| it == wrong), "offered `{wrong}`: {labels:?}");
    }
}

#[test]
fn prose_offers_nothing() {
    let source = "anchor A:\n    note: some prose here\n";
    let labels = labels_after(&[("main.pi", source)], "main.pi", "some prose ");
    assert!(labels.is_empty(), "{labels:?}");
}

#[test]
fn an_anchor_body_offers_the_properties_it_still_owes() {
    let labels = labels_after(&[("main.pi", KEYWORD_SOURCE)], "main.pi", "shape Concrete:\n    ");
    assert!(labels.iter().any(|it| it == "weight"), "{labels:?}");
    // Already written, so not offered again.
    assert!(!labels.iter().any(|it| it == "description"), "{labels:?}");
    // A key is not a value.
    assert!(!labels.iter().any(|it| it == "true"), "{labels:?}");
}

#[test]
fn an_empty_anchor_body_still_knows_which_anchor_it_is_in() {
    // There is no block node yet, so the anchor has to be found by looking back.
    let source = format!("{KEYWORD_SOURCE}\nshape Fresh:\n    ");
    let labels = labels_after(&[("main.pi", &source)], "main.pi", "shape Fresh:\n    ");
    assert!(labels.iter().any(|it| it == "description"), "{labels:?}");
    assert!(labels.iter().any(|it| it == "weight"), "{labels:?}");
}

#[test]
fn a_type_position_offers_types_and_extends_only_in_an_abstract() {
    let labels = labels_after(&[("main.pi", KEYWORD_SOURCE)], "main.pi", "description:: ");
    assert!(labels.iter().any(|it| it == "complex"), "{labels:?}");
    assert!(labels.iter().any(|it| it == "Shape"), "{labels:?}");
    assert!(labels.iter().any(|it| it == "extends"), "{labels:?}");

    let concrete = "anchor A:\n    thing:: string\n";
    let labels = labels_after(&[("main.pi", concrete)], "main.pi", "thing:: ");
    assert!(labels.iter().any(|it| it == "string"), "{labels:?}");
    assert!(!labels.iter().any(|it| it == "extends"), "`extends` is abstract-only: {labels:?}");
}

/// The full completion items at the position just after `marker`.
fn items_after(
    files: &[(&str, &str)],
    file: &str,
    marker: &str,
) -> Vec<tower_lsp::lsp_types::CompletionItem> {
    let (root, mut workspace) = workspace(files);
    let view = view_of(&mut workspace, root.join(file));
    let id = view.file_for(&root.join(file)).expect("file is analysed");
    let text = view.text(id).to_string();
    let at = text.find(marker).unwrap_or_else(|| panic!("{marker:?} not in:\n{text}"))
        + marker.len();
    completion::complete(&view, id, piton_syntax::TextSize::new(at as u32))
}

/// The text a completion item actually writes.
fn insert_text(item: &tower_lsp::lsp_types::CompletionItem) -> String {
    match &item.text_edit {
        Some(tower_lsp::lsp_types::CompletionTextEdit::Edit(edit)) => edit.new_text.clone(),
        _ => item.insert_text.clone().unwrap_or_else(|| item.label.clone()),
    }
}

const PATH_FILES: &[(&str, &str)] = &[
    ("piton.config.pi", "use @piton/config

export piton-config Config:
    root: .
"),
    ("main.pi", "from ./
"),
    ("Sibling.pi", "export anchor Sibling:
    x: 1
"),
    ("nested/index.pi", "from ./Inner export *
"),
    ("nested/Inner.pi", "export anchor Inner:
    x: 1
"),
    ("loose/Thing.pi", "export anchor Thing:
    x: 1
"),
];

#[test]
fn a_module_specifier_offers_real_files_and_directories() {
    let items = items_after(PATH_FILES, "main.pi", "from ./");
    let labels: Vec<String> = items.iter().map(|item| item.label.clone()).collect();

    // The label is the leaf, so the list reads like a file browser.
    assert!(labels.iter().any(|it| it == "Sibling"), "{labels:?}");
    // A directory that is importable shows as itself; one that is not shows
    // that there is more to type.
    assert!(labels.iter().any(|it| it == "nested"), "{labels:?}");
    assert!(labels.iter().any(|it| it == "loose/"), "{labels:?}");
    // `index.pi` is the directory, never a specifier of its own, and a file
    // cannot import itself.
    assert!(!labels.iter().any(|it| it.contains("index")), "{labels:?}");
    assert!(!labels.iter().any(|it| it == "main"), "{labels:?}");

    // The detail says what will be written, so the choice is obvious.
    let sibling = items.iter().find(|item| item.label == "Sibling").unwrap();
    let detail = sibling.detail.clone().unwrap_or_default();
    assert!(detail.contains("./Sibling"), "{detail}");
    assert!(detail.contains("module"), "{detail}");
    assert_eq!(insert_text(sibling), "Sibling", "only the leaf is replaced");

    // A plain directory completes with its separator, ready to continue.
    let loose = items.iter().find(|item| item.label == "loose/").unwrap();
    assert_eq!(insert_text(loose), "loose/");
}

#[test]
fn an_empty_specifier_offers_every_way_of_addressing_a_module() {
    let mut files = PATH_FILES.to_vec();
    files[1] = ("main.pi", "from \n");
    let items = items_after(&files, "main.pi", "from ");
    let details: Vec<String> =
        items.iter().map(|item| item.detail.clone().unwrap_or_default()).collect();

    // Relative, so the file beside this one is reachable.
    assert!(details.iter().any(|it| it.starts_with("./Sibling")), "{details:?}");
    // The project root, which is otherwise not discoverable at all.
    assert!(
        details.iter().any(|it| it.starts_with("/Sibling") && it.contains("project root")),
        "{details:?}"
    );
    // And the modules built into the compiler.
    assert!(details.iter().any(|it| it.starts_with("@piton/config")), "{details:?}");

    // With nothing typed, the candidate supplies its own prefix.
    let root_entry = items
        .iter()
        .find(|item| item.detail.as_deref().is_some_and(|it| it.starts_with("/Sibling")))
        .unwrap();
    assert_eq!(insert_text(root_entry), "/Sibling");

    // Relative first, then root, then builtin.
    let order: Vec<&str> = items
        .iter()
        .filter_map(|item| item.sort_text.as_deref())
        .map(|it| &it[..1])
        .collect();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(order, sorted, "the list is grouped by how it addresses the module");
}

#[test]
fn a_root_relative_specifier_lists_the_libraries_too() {
    // A library is not discoverable by reading the root, so a rooted specifier
    // has to offer it by name or nothing ever will.
    let files = &[
        (
            "piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n\n    \
             libraries:\n        customLib: ./lib\n",
        ),
        ("spec/main.pi", "from /\n"),
        ("spec/Sibling.pi", "export anchor Sibling:\n    x: 1\n"),
        ("lib/index.pi", "export anchor Tool:\n    x: 1\n"),
    ];
    let items = items_after(files, "spec/main.pi", "from /");
    let labels: Vec<String> = items.iter().map(|item| item.label.clone()).collect();
    assert!(labels.iter().any(|it| it == "Sibling"), "{labels:?}");
    assert!(labels.iter().any(|it| it == "customLib"), "{labels:?}");
}

/// Two sibling projects and one directory they both draw on, reached as `//`.
///
/// The shape a workspace takes once two projects share code without either
/// owning it: neither names the other, and the shared directory is spelled the
/// same way from both.
const SHARED_ROOT: &[(&str, &str)] = &[
    ("shared/Tool.pi", "export abstract anchor Tool as tool:\n    purpose:: string\n"),
    ("shared/index.pi", "from ./Tool export *\n"),
    (
        "origin/piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n\n    \
         sharedRoot: ../shared\n",
    ),
    (
        "origin/spec/index.pi",
        "use //Tool\n\nexport tool Hammer:\n    purpose: drive nails\n",
    ),
    (
        "substrate/piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./\n\n    \
         sharedRoot: ../shared\n",
    ),
    ("substrate/index.pi", "use //Tool\n\nexport tool Wrench:\n    purpose: turn bolts\n"),
];

#[test]
fn one_shared_root_serves_both_projects_that_name_it() {
    let (root, mut workspace) = workspace(SHARED_ROOT);
    for project in ["origin/spec/index.pi", "substrate/index.pi"] {
        let view = view_of(&mut workspace, root.join(project));
        let messages: Vec<&str> =
            view.compilation.diagnostics.iter().map(|it| it.message.as_str()).collect();
        assert!(messages.is_empty(), "{project}: {messages:?}");
        assert!(view.file_for(&root.join("shared/Tool.pi")).is_some(), "{project}");
    }
}

#[test]
fn a_shared_root_belongs_only_to_the_project_that_named_it() {
    let mut files = SHARED_ROOT.to_vec();
    // substrate stops naming it, and keeps the import that needed it.
    files[4] = (
        "substrate/piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./\n",
    );
    let (root, mut workspace) = workspace(&files);
    let view = view_of(&mut workspace, root.join("substrate/index.pi"));

    let messages: Vec<&str> =
        view.compilation.diagnostics.iter().map(|it| it.message.as_str()).collect();
    assert!(
        messages.iter().any(|it| it.contains("sharedRoot")),
        "the message should name the setting that is missing: {messages:?}"
    );
    assert!(
        view.file_for(&root.join("shared/Tool.pi")).is_none(),
        "a shared root nobody named is not in scope"
    );
}

#[test]
fn a_shared_root_that_is_not_there_is_reported_on_the_config() {
    let (root, mut workspace) = workspace(&[
        (
            "piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n\n    \
             sharedRoot: ../shared\n",
        ),
        ("spec/main.pi", "from //Tool import Tool\n"),
    ]);
    let config = root.join("piton.config.pi");
    let view = view_of(&mut workspace, &config);

    let diagnostics: Vec<_> = view.compilation.diagnostics.iter().collect();
    assert_eq!(diagnostics.len(), 1, "one cause, one diagnostic: {diagnostics:?}");
    assert_eq!(view.file_for(&config), Some(diagnostics[0].file));
    assert!(diagnostics[0].message.contains("`sharedRoot` is `../shared`"), "{diagnostics:?}");
}

#[test]
fn a_shared_specifier_lists_the_shared_root() {
    let files = &[
        (
            "piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n\n    \
             sharedRoot: ./shared\n",
        ),
        ("spec/main.pi", "from //\n"),
        ("spec/Sibling.pi", "export anchor Sibling:\n    x: 1\n"),
        ("shared/Tool.pi", "export anchor Tool:\n    x: 1\n"),
    ];
    let items = items_after(files, "spec/main.pi", "from //");
    let labels: Vec<String> = items.iter().map(|item| item.label.clone()).collect();
    assert!(labels.iter().any(|it| it == "Tool"), "{labels:?}");
    assert!(
        !labels.iter().any(|it| it == "Sibling"),
        "the root is not the shared root: {labels:?}"
    );
    for item in &items {
        assert!(
            item.detail.as_deref().is_some_and(|it| it.contains("shared root")),
            "{:?}",
            item.detail
        );
    }
}

#[test]
fn a_single_slash_offers_the_shared_root_before_it_is_typed() {
    // `//` is one keystroke past `/`, and a specifier that stopped at `/` is
    // the only place the second slash can be suggested.
    let files = &[
        (
            "piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n\n    \
             sharedRoot: ./shared\n",
        ),
        ("spec/main.pi", "from /\n"),
        ("spec/Sibling.pi", "export anchor Sibling:\n    x: 1\n"),
        ("shared/Tool.pi", "export anchor Tool:\n    x: 1\n"),
    ];
    let items = items_after(files, "spec/main.pi", "from /");
    let shared: Vec<&str> = items
        .iter()
        .filter(|item| item.detail.as_deref().is_some_and(|it| it.contains("shared root")))
        .map(|item| item.label.as_str())
        .collect();
    assert_eq!(shared, ["Tool"], "the shared root is offered beside the project root");
}

#[test]
fn a_root_relative_specifier_lists_the_project_root() {
    let mut files = PATH_FILES.to_vec();
    files[1] = ("main.pi", "from /\n");
    let items = items_after(&files, "main.pi", "from /");
    let labels: Vec<String> = items.iter().map(|item| item.label.clone()).collect();
    assert!(labels.iter().any(|it| it == "Sibling"), "{labels:?}");
    assert!(labels.iter().any(|it| it == "nested"), "{labels:?}");
    for item in &items {
        assert!(
            item.detail.as_deref().is_some_and(|it| it.contains("project root")),
            "{:?}",
            item.detail
        );
    }
}

#[test]
fn a_deeper_specifier_replaces_only_its_last_segment() {
    let mut files = PATH_FILES.to_vec();
    files[1] = ("main.pi", "from ./nested/In\n");
    let items = items_after(&files, "main.pi", "from ./nested/In");
    let inner = items.iter().find(|item| item.label == "Inner").expect("Inner is offered");
    assert_eq!(insert_text(inner), "Inner", "the directory already typed is left alone");
    assert!(
        inner.detail.as_deref().is_some_and(|it| it.starts_with("./nested/Inner")),
        "{:?}",
        inner.detail
    );
}

#[test]
fn a_builtin_module_specifier_offers_builtin_modules() {
    let labels = labels_after(&[("main.pi", "use @\n")], "main.pi", "use @");
    assert!(labels.iter().any(|it| it == "@piton/config"), "{labels:?}");
}

#[test]
fn an_import_list_offers_what_the_module_exports() {
    let files = &[
        ("main.pi", "from ./values import \n"),
        ("values.pi", "export shown: 1\nhidden: 2\nexport anchor Thing:\n    x: 1\n"),
    ];
    let labels = labels_after(files, "main.pi", "import ");
    assert!(labels.iter().any(|it| it == "shown"), "{labels:?}");
    assert!(labels.iter().any(|it| it == "Thing"), "{labels:?}");
    assert!(!labels.iter().any(|it| it == "hidden"), "unexported: {labels:?}");
}

#[test]
fn a_dot_inside_an_expression_offers_members() {
    let source = "\
anchor Base:
    name: Base
    weight: 1

anchor Child extends Base:
    detail: {self.}
    other: {Base.}
    nested: {Base.name}
";
    let labels = labels_after(&[("main.pi", source)], "main.pi", "{self.");
    assert!(labels.iter().any(|it| it == "name"), "{labels:?}");
    assert!(labels.iter().any(|it| it == "weight"), "{labels:?}");
    assert!(!labels.iter().any(|it| it == "Base"), "members only: {labels:?}");

    let labels = labels_after(&[("main.pi", source)], "main.pi", "{Base.");
    assert!(labels.iter().any(|it| it == "name"), "{labels:?}");
}

#[test]
fn an_expression_offers_names_and_self_references() {
    let source = "anchor A:\n    x: 1\n    y: {}\n";
    let labels = labels_after(&[("main.pi", source)], "main.pi", "y: {");
    assert!(labels.iter().any(|it| it == "A"), "{labels:?}");
    assert!(labels.iter().any(|it| it == "self"), "{labels:?}");
    // The old cross-product of `self.<every property>` is gone; `.` handles it.
    assert!(!labels.iter().any(|it| it.starts_with("self.")), "{labels:?}");
}

#[test]
fn semantic_tokens_cover_the_document() {
    let (root, mut workspace) = workspace(&[("shapes.pi", SHAPES), ("main.pi", MAIN)]);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let file = view.file_for(&root.join("main.pi")).unwrap();
    let produced = tokens::semantic_tokens(&view, file);
    assert!(produced.len() > 10, "expected a token per meaningful word: {}", produced.len());
    assert!(produced.iter().all(|token| (token.token_type as usize) < tokens::TOKEN_TYPES.len()));
}

#[test]
fn symbols_folding_links_and_hints() {
    let (root, mut workspace) = workspace(&[("shapes.pi", SHAPES), ("main.pi", MAIN)]);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let file = view.file_for(&root.join("main.pi")).unwrap();

    let symbols = tokens::document_symbols(&view, file);
    assert!(symbols.iter().any(|symbol| symbol.name == "Child"));
    assert!(symbols.iter().any(|symbol| symbol.name == "pi"));
    let child = symbols.iter().find(|symbol| symbol.name == "Child").unwrap();
    assert!(child.children.as_ref().is_some_and(|it| it.iter().any(|c| c.name == "detail")));

    assert!(!tokens::folding_ranges(&view, file).is_empty());

    let links = tokens::document_links(&view, file);
    assert_eq!(links.len(), 1, "the import path should be a link");

    let hints = tokens::inlay_hints(&view, file);
    assert!(hints.iter().any(|hint| match &hint.label {
        tower_lsp::lsp_types::InlayHintLabel::String(text) => text == ":: number",
        _ => false,
    }), "expected an inferred type for `pi`");
}

#[test]
fn code_actions_implement_missing_properties() {
    let source = "\
abstract anchor Shape as shape:
    description:: string
    weight:: number

shape Concrete:
    other: value
";
    let (root, mut workspace) = workspace(&[("main.pi", source)]);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let file = view.file_for(&root.join("main.pi")).unwrap();
    let url = Url::from_file_path(root.join("main.pi")).unwrap();
    let range = piton_syntax::TextRange::new(
        offset_of(source, "shape Concrete"),
        offset_of(source, "shape Concrete") + piton_syntax::TextSize::new(14),
    );
    let offered = actions::actions(&view, file, &url, range);
    let titles: Vec<String> = offered
        .iter()
        .filter_map(|action| match action {
            tower_lsp::lsp_types::CodeActionOrCommand::CodeAction(action) => {
                Some(action.title.clone())
            }
            _ => None,
        })
        .collect();
    assert!(titles.iter().any(|it| it.contains("Implement 2 missing")), "{titles:?}");
    assert!(titles.iter().any(|it| it.contains("Format")), "{titles:?}");
}

#[test]
fn positions_survive_multi_byte_characters() {
    let source = "note: héllo wörld ${x}\nx: 1\n";
    let (root, mut workspace) = workspace(&[("main.pi", source)]);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let file = view.file_for(&root.join("main.pi")).unwrap();
    let index = view.line_index(file);
    let at = index.offset(Position { line: 0, character: 20 });
    assert_eq!(&source[usize::from(at)..usize::from(at) + 1], "x");
    assert_eq!(index.position(at), Position { line: 0, character: 20 });
}

// ---- the features that were only smoke-tested before ------------------------

/// Decode the LSP's delta encoding back into something readable.
fn decoded_tokens(view: &View, file: FileId) -> Vec<(u32, u32, String, String)> {
    let text = view.text(file);
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let (mut line, mut start) = (0u32, 0u32);
    for token in tokens::semantic_tokens(view, file) {
        line += token.delta_line;
        start = if token.delta_line == 0 { start + token.delta_start } else { token.delta_start };
        let source = lines
            .get(line as usize)
            .map(|it| {
                it.chars()
                    .skip(start as usize)
                    .take(token.length as usize)
                    .collect::<String>()
            })
            .unwrap_or_default();
        let kind = tokens::TOKEN_TYPES[token.token_type as usize].as_str().to_string();
        out.push((line, start, source, kind));
    }
    out
}

#[test]
fn semantic_tokens_classify_each_construct() {
    let source = "\
export abstract anchor Shape as shape:
    description:: string

shape Concrete:
    description: plain prose here
    count: 42
    flag: true
    computed: ${description}
";
    let (root, mut workspace) = workspace(&[("main.pi", source)]);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let file = view.file_for(&root.join("main.pi")).unwrap();
    let produced = decoded_tokens(&view, file);
    let kind_of = |text: &str| {
        produced
            .iter()
            .find(|(_, _, source, _)| source == text)
            .map(|(_, _, _, kind)| kind.clone())
            .unwrap_or_else(|| panic!("no token for {text:?} in {produced:#?}"))
    };

    assert_eq!(kind_of("anchor"), "keyword");
    assert_eq!(kind_of("abstract"), "keyword");
    assert_eq!(kind_of("Shape"), "class", "an anchor name is a type definition");
    assert_eq!(kind_of("shape"), "function", "a user keyword reads like a constructor");
    assert_eq!(kind_of("string"), "type");
    assert_eq!(kind_of("description"), "property");
    assert_eq!(kind_of("42"), "number");
    assert_eq!(kind_of("true"), "enumMember", "a literal is not a keyword");
    assert_eq!(kind_of("prose"), "string", "prose is a string, not an identifier");
    assert_eq!(kind_of("$"), "decorator", "the interpolation sigil stands out");
}

#[test]
fn an_interpolation_is_delimited_the_same_at_both_ends() {
    let source = "x: Hello ${name} and @{Other}\ny: {a + b}\n";
    let (root, mut workspace) = workspace(&[("main.pi", source)]);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let file = view.file_for(&root.join("main.pi")).unwrap();
    let produced = decoded_tokens(&view, file);

    let braces: Vec<(String, String)> = produced
        .iter()
        .filter(|(_, _, text, _)| text == "{" || text == "}")
        .map(|(_, _, text, kind)| (text.clone(), kind.clone()))
        .collect();
    assert_eq!(braces.len(), 6, "three interpolations, two braces each: {produced:#?}");
    // Whatever they are, both ends have to agree.
    let opening = &braces[0].1;
    for (text, kind) in &braces {
        assert_eq!(kind, opening, "the `{text}` of an interpolation differs from its partner");
    }
}

#[test]
fn semantic_tokens_mark_a_declaration_as_one() {
    let source = "abstract anchor Shape:\n    x:: string\n";
    let (root, mut workspace) = workspace(&[("main.pi", source)]);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let file = view.file_for(&root.join("main.pi")).unwrap();
    let declaration = tokens::semantic_tokens(&view, file)
        .into_iter()
        .find(|token| tokens::TOKEN_TYPES[token.token_type as usize].as_str() == "class")
        .expect("the anchor name is classified");
    // declaration | definition | abstract
    assert_ne!(declaration.token_modifiers_bitset & 0b0001, 0, "declaration");
    assert_ne!(declaration.token_modifiers_bitset & 0b1000, 0, "abstract");
}

#[test]
fn definition_works_from_every_kind_of_reference() {
    let files = &[
        (
            "main.pi",
            "\
use ./keywords

from ./shapes import Base

anchor Child extends Base:
    typed:: Base
    value: {Base.name}
",
        ),
        ("shapes.pi", "export anchor Base:\n    name: Base\n"),
        ("keywords.pi", "export abstract anchor K as kw:\n    x:: string\n"),
    ];
    let (root, mut workspace) = workspace(files);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let main = view.file_for(&root.join("main.pi")).unwrap();
    let shapes = view.file_for(&root.join("shapes.pi")).unwrap();
    let keywords = view.file_for(&root.join("keywords.pi")).unwrap();
    let source = view.text(main).to_string();

    let jump = |needle: &str| {
        let at = piton_syntax::TextSize::new(source.find(needle).expect(needle) as u32);
        let resolved = navigation::resolve(&view, main, at)
            .unwrap_or_else(|| panic!("nothing resolves at {needle:?}"));
        navigation::definition(&view, &resolved.target)
            .unwrap_or_else(|| panic!("no definition for {needle:?}"))
    };

    // The imported name, the base, the type, and the expression reference all
    // lead to the same declaration.
    for needle in ["Base\n", "Base:\n", "Base\n    value", "Base.name"] {
        assert_eq!(jump(needle).0, shapes, "from {needle:?}");
    }
    // A module path leads to the file.
    assert_eq!(jump("./keywords").0, keywords);
    assert_eq!(jump("./shapes").0, shapes);
}

#[test]
fn rename_touches_every_reference_and_the_declaration() {
    let files = &[
        ("main.pi", "from ./shapes import Base\n\nanchor Child extends Base:\n    x: 1\n"),
        ("shapes.pi", "export anchor Base:\n    name: Base\n"),
    ];
    let (root, mut workspace) = workspace(files);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let main = view.file_for(&root.join("main.pi")).unwrap();
    let shapes = view.file_for(&root.join("shapes.pi")).unwrap();
    let source = view.text(main).to_string();
    let at = piton_syntax::TextSize::new(source.find("Base:").unwrap() as u32);

    let resolved = navigation::resolve(&view, main, at).expect("the base resolves");
    let mut sites = navigation::references(&view, &resolved.target);
    sites.push(navigation::definition(&view, &resolved.target).expect("a declaration"));
    sites.sort_by_key(|(file, range)| (file.0, u32::from(range.start())));
    sites.dedup();

    assert!(sites.iter().any(|(file, _)| *file == shapes), "the declaration is renamed");
    assert!(sites.iter().filter(|(file, _)| *file == main).count() >= 2, "import and extends");
    // The `Base` inside the prose value `name: Base` is not a reference.
    for (file, range) in &sites {
        assert_eq!(&view.text(*file)[*range], "Base");
    }
}

#[test]
fn hover_covers_modules_builtins_and_properties() {
    let files = &[
        ("main.pi", "from ./shapes import Base\n\nanchor Child extends Base:\n    typed:: string: x\n"),
        ("shapes.pi", "export anchor Base:\n    name: Base\n"),
    ];
    let (root, mut workspace) = workspace(files);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let main = view.file_for(&root.join("main.pi")).unwrap();
    let source = view.text(main).to_string();

    let text_at = |needle: &str| {
        let at = piton_syntax::TextSize::new(source.find(needle).expect(needle) as u32);
        let resolved = navigation::resolve(&view, main, at)?;
        hover::hover(&view, &resolved)
    };

    let module = text_at("./shapes").expect("a module hovers");
    assert!(module.contains("Exports"), "{module}");
    assert!(module.contains("Base"), "{module}");

    let builtin = text_at("string").expect("a built-in type hovers");
    assert!(builtin.to_lowercase().contains("string"), "{builtin}");

    let property = text_at("typed").expect("a property hovers");
    assert!(property.contains("typed"), "{property}");
}

#[test]
fn the_type_hierarchy_walks_both_ways() {
    let source = "\
abstract anchor Root:
    x:: string

abstract anchor Middle extends Root:
    y:: string

anchor Leaf extends Middle:
    x: 1
    y: 2
";
    let (root, mut workspace) = workspace(&[("main.pi", source)]);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let file = view.file_for(&root.join("main.pi")).unwrap();
    let analysis = &view.compilation.analysis;
    let id_of = |name: &str| {
        analysis
            .anchor_ids()
            .find(|id| analysis.anchor_def(*id).name == name)
            .unwrap_or_else(|| panic!("{name} is declared"))
    };
    let _ = file;

    let leaf = id_of("Leaf");
    let middle = id_of("Middle");
    let root_anchor = id_of("Root");
    assert_eq!(analysis.bases(leaf), vec![middle], "supertypes are the direct bases");
    assert_eq!(analysis.subtypes(middle), vec![leaf], "subtypes name it directly");
    assert!(analysis.ancestors(leaf).contains(&root_anchor), "the chain reaches the root");
    // Only concrete anchors implement.
    assert_eq!(analysis.implementors(root_anchor), vec![leaf]);
}

#[test]
fn code_actions_offer_the_import_and_the_use_that_are_missing() {
    let files = &[
        ("main.pi", "broken: {Thing}\n\nunknown-kw Other:\n    x: 1\n"),
        ("shapes.pi", "export anchor Thing:\n    x: 1\n"),
        ("keywords.pi", "export abstract anchor K as unknown-kw:\n    x:: string\n"),
    ];
    let (root, mut workspace) = workspace(files);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let file = view.file_for(&root.join("main.pi")).unwrap();
    let url = Url::from_file_path(root.join("main.pi")).unwrap();
    let whole = piton_syntax::TextRange::new(
        piton_syntax::TextSize::new(0),
        piton_syntax::TextSize::new(view.text(file).len() as u32),
    );
    let titles: Vec<String> = actions::actions(&view, file, &url, whole)
        .iter()
        .filter_map(|action| match action {
            tower_lsp::lsp_types::CodeActionOrCommand::CodeAction(action) => {
                Some(action.title.clone())
            }
            _ => None,
        })
        .collect();
    assert!(
        titles.iter().any(|it| it.contains("Import `Thing`") && it.contains("./shapes")),
        "{titles:?}"
    );
    assert!(
        titles.iter().any(|it| it.contains("use ./keywords")),
        "{titles:?}"
    );
}

#[test]
fn formatting_only_reports_edits_when_there_are_any() {
    let tidy = "anchor A:\n    name: value\n";
    let (tidy_root, mut tidy_workspace) = workspace(&[("main.pi", tidy)]);
    let view = view_of(&mut tidy_workspace, tidy_root.join("main.pi"));
    let file = view.file_for(&tidy_root.join("main.pi")).unwrap();
    assert_eq!(piton_fmt::format(view.text(file)), tidy, "already canonical");

    let untidy = "anchor A:\n  name: value\n";
    let (other_root, mut other) = workspace(&[("main.pi", untidy)]);
    let view = view_of(&mut other, other_root.join("main.pi"));
    let file = view.file_for(&other_root.join("main.pi")).unwrap();
    let formatted = piton_fmt::format(view.text(file));
    assert_ne!(formatted, untidy);
    assert_eq!(formatted, tidy);
}

#[test]
fn document_links_point_at_the_files_they_name() {
    let files = &[
        ("main.pi", "use ./keywords\n\nfrom ./shapes import Base\n"),
        ("shapes.pi", "export anchor Base:\n    x: 1\n"),
        ("keywords.pi", "export abstract anchor K as kw:\n    x:: string\n"),
    ];
    let (root, mut workspace) = workspace(files);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let file = view.file_for(&root.join("main.pi")).unwrap();
    let links = tokens::document_links(&view, file);
    assert_eq!(links.len(), 2, "one per path");
    let targets: Vec<String> =
        links.iter().filter_map(|link| link.target.as_ref()).map(|it| it.to_string()).collect();
    assert!(targets.iter().any(|it| it.ends_with("keywords.pi")), "{targets:?}");
    assert!(targets.iter().any(|it| it.ends_with("shapes.pi")), "{targets:?}");
}

#[test]
fn inlay_hints_appear_only_where_no_constraint_was_written() {
    let source = "\
plain: 42
typed:: number: 42

anchor A:
    inferred: some prose
    declared:: string: more prose
";
    let (root, mut workspace) = workspace(&[("main.pi", source)]);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let file = view.file_for(&root.join("main.pi")).unwrap();
    let index = view.line_index(file);
    let hints: Vec<(u32, String)> = tokens::inlay_hints(&view, file)
        .into_iter()
        .map(|hint| match hint.label {
            tower_lsp::lsp_types::InlayHintLabel::String(text) => (hint.position.line, text),
            _ => (hint.position.line, String::new()),
        })
        .collect();
    let _ = index;
    assert!(hints.iter().any(|(line, text)| *line == 0 && text == ":: number"), "{hints:?}");
    assert!(hints.iter().any(|(line, text)| *line == 4 && text == ":: string"), "{hints:?}");
    // A written constraint needs no hint.
    assert!(!hints.iter().any(|(line, _)| *line == 1), "{hints:?}");
    assert!(!hints.iter().any(|(line, _)| *line == 5), "{hints:?}");
}

#[test]
fn workspace_symbols_find_declarations_across_files() {
    let files = &[
        ("main.pi", "anchor ButtonThing:\n    x: 1\n"),
        ("other.pi", "buttonCount: 3\nanchor Unrelated:\n    y: 2\n"),
    ];
    let (root, mut workspace) = workspace(files);
    let view = view_of(&mut workspace, root.join("main.pi"));
    let mut found: Vec<String> = Vec::new();
    for file in view.compilation.analysis.db.files() {
        for anchor in &file.hir.anchors {
            if anchor.name.to_lowercase().contains("button") {
                found.push(anchor.name.clone());
            }
        }
        for variable in &file.hir.vars {
            if variable.name.to_lowercase().contains("button") {
                found.push(variable.name.clone());
            }
        }
    }
    found.sort();
    assert_eq!(found, vec!["ButtonThing".to_string(), "buttonCount".to_string()]);
    let _ = root;
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
