//! A test for each rule of `spec/scope/lsp/model`, and for the features built
//! on the symbol model: what a name resolves to, what renaming it changes,
//! what hovering it says, and what imports follow from it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use piton_core::framework::Frameworks;
use piton_core::FileId;
use piton_syntax::{TextRange, TextSize};
use tower_lsp::lsp_types::{CodeActionOrCommand, TextEdit, Url, WorkspaceEdit};

use crate::index::{Located, Sym};
use crate::line_index::LineIndex;
use crate::world::{View, Workspace};
use crate::{actions, completion, hover, imports, navigation, refactor, tokens};

fn project(files: &[(&str, &str)]) -> (PathBuf, Workspace) {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let ordinal = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("piton-lsp-model-{}-{ordinal}", std::process::id()));
    for (path, contents) in files {
        let target = root.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, contents).unwrap();
    }
    let mut workspace = Workspace::new(Frameworks::default as fn() -> Frameworks);
    workspace.set_roots(vec![root.clone()]);
    (root, workspace)
}

fn view_of(workspace: &mut Workspace, path: &Path) -> Arc<View> {
    let snapshot = workspace.snapshot();
    let (view, _) = snapshot.locate(path).unwrap_or_else(|| panic!("no project analysed {}", path.display()));
    view.clone()
}

fn file_of(view: &View, path: &Path) -> FileId {
    view.file_for(path).unwrap_or_else(|| panic!("{} is not analysed", path.display()))
}

fn offset(view: &View, file: FileId, needle: &str) -> TextSize {
    let at = view.text(file).find(needle).unwrap_or_else(|| panic!("{needle:?} is not in the file"));
    TextSize::new(at as u32)
}

fn locate(view: &View, file: FileId, needle: &str) -> Located {
    navigation::locate(view, file, offset(view, file, needle)).unwrap_or_else(|| panic!("nothing at {needle:?}"))
}

fn sym(located: &Located) -> Sym {
    match located {
        Located::Symbol(occurrence) => occurrence.sym.clone(),
        other => panic!("not a symbol: {other:?}"),
    }
}

fn line(view: &View, (file, range): (FileId, TextRange)) -> u32 {
    view.line_index(file).position(range.start()).line
}

/// Apply edits to a text the way an editor does.
fn apply(text: &str, mut edits: Vec<TextEdit>) -> String {
    edits.sort_by_key(|edit| std::cmp::Reverse((edit.range.start.line, edit.range.start.character)));
    let mut out = text.to_string();
    for edit in edits {
        let index = LineIndex::new(&out);
        let start = u32::from(index.offset(edit.range.start)) as usize;
        let end = u32::from(index.offset(edit.range.end)) as usize;
        out.replace_range(start..end, &edit.new_text);
    }
    out
}

/// Every file a workspace edit changes, with its text afterwards.
fn applied(edit: WorkspaceEdit) -> HashMap<PathBuf, String> {
    edit.changes
        .unwrap_or_default()
        .into_iter()
        .map(|(url, edits)| {
            let path = url.to_file_path().expect("a file url");
            let text = std::fs::read_to_string(&path).expect("an edited file exists");
            (path, apply(&text, edits))
        })
        .collect()
}

/// Rename the symbol at `needle` in `path`.
fn rename(workspace: &mut Workspace, path: &Path, needle: &str, name: &str) -> Result<HashMap<PathBuf, String>, String> {
    let snapshot = workspace.snapshot();
    let (view, file) = snapshot.locate(path).expect("analysed");
    let at = offset(view, file, needle);
    refactor::rename_edits(&snapshot, path, at, name).map(applied)
}

/// The titles of the code actions offered anywhere in a file.
fn action_titles(view: &View, file: FileId, path: &Path) -> Vec<(String, WorkspaceEdit)> {
    let url = Url::from_file_path(path).unwrap();
    let whole = TextRange::new(TextSize::new(0), TextSize::new(view.text(file).len() as u32));
    actions::actions(view, file, &url, whole, None)
        .into_iter()
        .filter_map(|action| match action {
            CodeActionOrCommand::CodeAction(action) => Some((action.title, action.edit.unwrap_or_default())),
            CodeActionOrCommand::Command(_) => None,
        })
        .collect()
}

// ---- resolution ----------------------------------------------------------------

#[test]
fn a_member_access_resolves_against_the_anchor_before_the_dot() {
    let (root, mut workspace) = project(&[
        ("other.pi", "export anchor Thing:\n    x: 1\n"),
        (
            "main.pi",
            "from ./other import Thing\n\nanchor Child:\n    value: {Thing.x}\n    missing: {Thing.nope}\n",
        ),
    ]);
    let main_path = root.join("main.pi");
    let view = view_of(&mut workspace, &main_path);
    let main = file_of(&view, &main_path);
    let other = file_of(&view, &root.join("other.pi"));

    let access = locate(&view, main, "x}");
    let x = offset(&view, other, "x: 1");
    assert_eq!(navigation::definitions(&view, main, &access), [(other, TextRange::new(x, x + TextSize::new(1)))]);
    let text = hover::hover(&view, &access).expect("a hover");
    assert!(text.contains("Declared on `Thing`"), "{text}");
    // A property the anchor does not have resolves to nothing; the compiler
    // is the one that reports it.
    assert!(navigation::locate(&view, main, offset(&view, main, "nope")).is_none());
}

#[test]
fn self_and_this_hold_their_anchor_and_super_reads_the_bases() {
    let source = "\
anchor Base:
    name: Base

anchor Child extends Base:
    name: Child
    mine: ${self.name}
    theirs: ${super.name}
    exact: ${this.name}
";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let main = file_of(&view, &path);
    let lines_from = |needle: &str, skip: u32| -> Vec<u32> {
        let at = offset(&view, main, needle) + TextSize::new(skip);
        let located = navigation::locate(&view, main, at).expect("resolves");
        navigation::definitions(&view, main, &located).into_iter().map(|it| line(&view, it)).collect()
    };
    assert_eq!(lines_from("self.name", 5), [4], "`self.name` uses Child's own declaration");
    assert_eq!(lines_from("super.name", 6), [1], "`super.name` uses what the base provides");
    assert_eq!(lines_from("this.name", 5), [4]);

    let itself = navigation::locate(&view, main, offset(&view, main, "self.name")).unwrap();
    assert!(matches!(itself, Located::SelfReference(_)), "{itself:?}");
    assert_eq!(lines_from("self.name", 0), [3], "`self` goes to the anchor it is written in");
    assert_eq!(lines_from("super.name", 0), [0], "`super` goes to the bases");
    assert!(navigation::highlights(&view, main, &itself).is_empty(), "`self` is nobody's reference");
}

#[test]
fn an_alias_is_a_symbol_of_its_own() {
    let tools = "export anchor Hammer:\n    weight: 1\n";
    let main = "from ./tools import Hammer Mallet\n\nheavy: {Mallet.weight}\nnote: Use a ${Mallet}.\n";
    let (root, mut workspace) = project(&[("tools.pi", tools), ("main.pi", main)]);
    let main_path = root.join("main.pi");
    let tools_path = root.join("tools.pi");
    let view = view_of(&mut workspace, &main_path);
    let main_id = file_of(&view, &main_path);
    let tools_id = file_of(&view, &tools_path);

    let usage = locate(&view, main_id, "Mallet.weight");
    assert_eq!(navigation::definitions(&view, main_id, &usage)[0].0, tools_id, "an alias goes to what it names");
    let text = hover::hover(&view, &usage).unwrap();
    assert!(text.contains("alias of `Hammer`"), "{text}");
    let weight = locate(&view, main_id, "weight}");
    assert_eq!(navigation::definitions(&view, main_id, &weight)[0].0, tools_id);

    let renamed = rename(&mut workspace, &tools_path, "Hammer", "Sledge").unwrap();
    assert_eq!(renamed[&tools_path], tools.replace("Hammer", "Sledge"));
    assert_eq!(renamed[&main_path], main.replace("import Hammer", "import Sledge"), "the alias stays");

    let renamed = rename(&mut workspace, &main_path, "Mallet.weight", "Maul").unwrap();
    assert_eq!(renamed.len(), 1, "only the file with the alias changes");
    assert_eq!(renamed[&main_path], main.replace("Mallet", "Maul"));
}

#[test]
fn an_import_follows_every_re_export_back_to_the_declaration() {
    let tool = "export anchor Tool:\n    x: 1\n";
    let main = "from ./tools import Tool\n\nvalue: {Tool.x}\n";
    for (index, index_after) in [
        ("from ./Tool export *\n", "from ./Tool export *\n"),
        ("from ./Tool export Tool\n", "from ./Tool export Gear\n"),
    ] {
        let (root, mut workspace) =
            project(&[("tools/Tool.pi", tool), ("tools/index.pi", index), ("main.pi", main)]);
        let main_path = root.join("main.pi");
        let view = view_of(&mut workspace, &main_path);
        let main_id = file_of(&view, &main_path);
        let imported = locate(&view, main_id, "Tool\n");
        let declared_in = file_of(&view, &root.join("tools/Tool.pi"));
        assert_eq!(navigation::definitions(&view, main_id, &imported)[0].0, declared_in);

        let renamed = rename(&mut workspace, &main_path, "Tool.x", "Gear").unwrap();
        assert_eq!(renamed[&root.join("tools/Tool.pi")], tool.replace("Tool", "Gear"));
        assert_eq!(renamed[&main_path], main.replace("Tool", "Gear"));
        let index_now = renamed.get(&root.join("tools/index.pi")).map(String::as_str).unwrap_or(index);
        assert_eq!(index_now, index_after);
    }
}

#[test]
fn a_built_in_type_is_a_symbol_no_rename_can_change() {
    let source = "abstract anchor Base:\n    x:: string\n    items:: extends Base[]\n";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let main = file_of(&view, &path);
    assert_eq!(sym(&locate(&view, main, "string")), Sym::Builtin("string".to_string()));
    assert!(matches!(sym(&locate(&view, main, "Base[]")), Sym::Anchor(_)));
    let refused = refactor::prepare_rename(&view, main, offset(&view, main, "string")).expect_err("refused");
    assert!(refused.contains("built-in"), "{refused}");
}

// ---- rename ----------------------------------------------------------------------

#[test]
fn renaming_a_property_follows_its_whole_family_and_nothing_else() {
    let source = "\
abstract anchor Shape as shape:
    name:: string

shape Square:
    name: Square

anchor First:
    description: first

anchor Second:
    description: second
    echo: ${self.description}

anchor Both extends First, Second:
    description: both

anchor Unrelated:
    description: unrelated

summary: {Both.description}
";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");

    let renamed = rename(&mut workspace, &path, "description: first", "detail").unwrap();
    let expected = source
        .replace("description: first", "detail: first")
        .replace("description: second", "detail: second")
        .replace("self.description", "self.detail")
        .replace("description: both", "detail: both")
        .replace("Both.description", "Both.detail");
    assert_eq!(renamed[&path], expected, "a merged base is the same property; an unrelated anchor is not");

    let renamed = rename(&mut workspace, &path, "name: Square", "title").unwrap();
    assert_eq!(renamed[&path], source.replace("name:: string", "title:: string").replace("name: Square", "title: Square"));
}

#[test]
fn a_nested_key_belongs_to_the_value_that_holds_it() {
    let source = "\
config:
    depth: 3
    inner:
        width: 2

a: {config.inner.width}
b: {config.depth}
";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let main = file_of(&view, &path);
    let width = locate(&view, main, "width}");
    let found: Vec<u32> = navigation::definitions(&view, main, &width).into_iter().map(|it| line(&view, it)).collect();
    assert_eq!(found, [3]);

    let renamed = rename(&mut workspace, &path, "width}", "size").unwrap();
    assert_eq!(renamed[&path], source.replace("width", "size"));
    let refused = rename(&mut workspace, &path, "depth}", "inner").expect_err("taken");
    assert!(refused.contains("already has a key"), "{refused}");
}

#[test]
fn a_rename_is_refused_for_a_name_that_is_not_one_or_is_taken() {
    let source = "\
anchor Tool:
    weight: 1
    size: 2

anchor Other:
    x: 1

abstract anchor Kind as kind:
    y:: string
";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");
    let mut refused = |needle: &str, name: &str| rename(&mut workspace, &path, needle, name).expect_err(name);
    assert!(refused("Tool", "Other").contains("already declared"));
    assert!(refused("Tool", "self").contains("reserved"));
    assert!(refused("Tool", "string").contains("built-in type"));
    assert!(refused("Tool", "two words").contains("not a name"));
    assert!(refused("weight", "size").contains("already has a property"));
    assert!(refused("kind:", "Kind").contains("lowercase"));
    assert!(rename(&mut workspace, &path, "kind:", "sort").is_ok());
    assert!(rename(&mut workspace, &path, "Tool", "Tool").unwrap().is_empty(), "the same name changes nothing");
}

#[test]
fn what_a_builtin_module_declares_cannot_be_renamed() {
    let (root, mut workspace) = project(&[
        ("piton.config.pi", "use @piton/config\n\nexport piton-config Config:\n    root: ./\n"),
        ("main.pi", "a: 1\n"),
    ]);
    let config = root.join("piton.config.pi");
    let view = view_of(&mut workspace, &config);
    let file = file_of(&view, &config);
    for needle in ["piton-config", "root"] {
        let refused = refactor::prepare_rename(&view, file, offset(&view, file, needle)).expect_err(needle);
        assert!(refused.contains("@piton/config"), "{needle}: {refused}");
    }
}

#[test]
fn references_reach_every_project_that_can_see_the_symbol() {
    let (root, mut workspace) = project(&[
        ("shared/Tool.pi", "export abstract anchor Tool as tool:\n    purpose:: string\n"),
        ("shared/index.pi", "from ./Tool export *\n"),
        (
            "origin/piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n\n    sharedRoot: ../shared\n",
        ),
        ("origin/spec/index.pi", "use //Tool\n\nexport tool Hammer:\n    purpose: drive nails\n"),
        (
            "substrate/piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./\n\n    sharedRoot: ../shared\n",
        ),
        ("substrate/index.pi", "use //Tool\n\nexport tool Wrench:\n    purpose: turn bolts\n"),
    ]);
    let snapshot = workspace.snapshot();
    let shared = root.join("shared/Tool.pi");
    let (view, file) = snapshot.locate(&shared).expect("analysed");
    let keyword = navigation::locate(view, file, offset(view, file, "tool:")).expect("the keyword");
    let mut found: Vec<String> = Vec::new();
    for other in snapshot.views() {
        for counterpart in navigation::counterparts(view, &sym(&keyword), other) {
            for (site, _) in navigation::references(other, &counterpart, false) {
                let Some(path) = other.path_of(site) else { continue };
                let name = path.strip_prefix(&root).unwrap().display().to_string();
                if !found.contains(&name) {
                    found.push(name);
                }
            }
        }
    }
    found.sort();
    assert_eq!(found, ["origin/spec/index.pi", "substrate/index.pi"]);
}

// ---- hover, implementations, and the hierarchy -------------------------------------------

#[test]
fn hover_says_where_a_property_comes_from_and_what_fills_in_a_shape() {
    let source = "\
abstract anchor Shape as shape:
    name:: string

shape Square:
    name: Square

anchor Base:
    label: base

anchor Child extends Base:
    label: child
";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let main = file_of(&view, &path);
    let text = |needle: &str| hover::hover(&view, &locate(&view, main, needle)).expect(needle);

    assert!(text("Shape as").contains("Implemented by `Square`"), "{}", text("Shape as"));
    let label = text("label: child");
    for expected in ["Declared on `Child`.", "Overrides `Base`.", "\"child\""] {
        assert!(label.contains(expected), "{expected} in {label}");
    }
    assert!(text("shape Square").contains("keyword `shape`"), "{}", text("shape Square"));
}

#[test]
fn a_property_is_implemented_where_a_concrete_anchor_writes_it() {
    let source = "\
abstract anchor Shape as shape:
    name:: string

shape Square:
    name: square

shape Circle:
    name: circle
";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let main = file_of(&view, &path);
    let mut found: Vec<u32> = navigation::implementations(&view, &locate(&view, main, "name::"))
        .into_iter()
        .map(|it| line(&view, it))
        .collect();
    found.sort();
    assert_eq!(found, [4, 7]);
}

#[test]
fn a_keyword_goes_to_its_as_clause_and_starts_the_hierarchy_at_its_anchor() {
    let source = "\
abstract anchor Shape as shape:
    name:: string

shape Square:
    name: square
    echo: ${self.name}
";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let main = file_of(&view, &path);
    let keyword = locate(&view, main, "shape Square");
    let (file, range) = navigation::definitions(&view, main, &keyword)[0];
    assert_eq!((line(&view, (file, range)), &view.text(file)[range]), (0, "shape"));

    let analysis = &view.compilation.analysis;
    let name = |needle: &str| {
        let id = navigation::anchor_at(&view, main, offset(&view, main, needle)).expect(needle);
        analysis.anchor_def(id).name.clone()
    };
    assert_eq!(name("shape Square"), "Shape");
    assert_eq!(name("self"), "Square");
}

// ---- imports ------------------------------------------------------------------------------

#[test]
fn an_import_is_unused_only_when_nothing_in_the_file_resolves_to_it() {
    let parts = "\
export anchor Gear:
    x: 1

export anchor Spring:
    x: 1

export anchor Lever:
    x: 1

export anchor Idle:
    x: 1

export abstract anchor Part as part:
    q:: string
";
    let kinds = "export abstract anchor Kind as kind:\n    s:: string\n";
    let main = "\
use ./parts
use ./kinds

from ./parts import Gear, Spring, Lever Pry, Idle
from ./parts export Lever

export Spring

part Wheel:
    q: round

note: A ${Gear} and more.
";
    let broken = format!("{main}x::number: 42\n");
    let (root, mut workspace) =
        project(&[("parts.pi", parts), ("kinds.pi", kinds), ("main.pi", main), ("broken.pi", &broken)]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let main_id = file_of(&view, &path);
    let mut titles: Vec<String> = imports::unused(&view, main_id).into_iter().map(|it| it.title).collect();
    titles.sort();
    assert_eq!(
        titles,
        ["Remove unused `use ./kinds`", "Remove unused import `Idle`", "Remove unused import `Pry`"],
        "prose, `export`, and a keyword each count as a use, and a re-export is never unused"
    );
    let broken_id = file_of(&view, &root.join("broken.pi"));
    assert!(imports::unused(&view, broken_id).is_empty(), "a file that does not parse reports none");
}

#[test]
fn removing_unused_imports_keeps_everything_else_as_it_was_written() {
    let parts = "export anchor A:\n    x: 1\n\nexport anchor B:\n    x: 1\n\nexport anchor C:\n    x: 1\n";
    let more = "export anchor D:\n    x: 1\n\nexport anchor E:\n    x: 1\n\nexport anchor F:\n    x: 1\n";
    let main = "from ./parts import A, B, C\nfrom ./more import\n    D,\n    E,\n    F\n\nx: {A}\ny: {C}\nz: {E}\n";
    let lone = "from ./parts import B\n\nx: 1\n";
    let (root, mut workspace) = project(&[("parts.pi", parts), ("more.pi", more), ("main.pi", main), ("lone.pi", lone)]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let main_id = file_of(&view, &path);

    let unused = imports::unused(&view, main_id);
    assert_eq!(
        apply(main, imports::removal(&view, main_id, &unused)),
        "from ./parts import A, C\nfrom ./more import\n    E\n\nx: {A}\ny: {C}\nz: {E}\n"
    );
    assert_eq!(
        apply(main, imports::organize(&view, main_id)),
        "from ./parts import A, C\nfrom ./more import E\n\nx: {A}\ny: {C}\nz: {E}\n"
    );

    let lone_id = file_of(&view, &root.join("lone.pi"));
    let unused = imports::unused(&view, lone_id);
    assert_eq!(apply(lone, imports::removal(&view, lone_id, &unused)), "\nx: 1\n");
}

#[test]
fn an_import_is_added_from_the_shortest_specifier_the_rules_allow() {
    let point = "export anchor PointPrimitive:\n    x: 1\n\nexport anchor Extra:\n    x: 1\n";
    let joined = "from ../point import Extra\n\nanchor Joined:\n    p: {PointPrimitive.x}\n    e: {Extra.x}\n";
    let (root, mut workspace) = project(&[
        ("piton.config.pi", "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n"),
        ("spec/scope/point/Point.pi", point),
        ("spec/scope/point/index.pi", "from ./Point export *\n"),
        ("spec/scope/point/Line.pi", "anchor Line:\n    p: {PointPrimitive.x}\n"),
        ("spec/scope/arc/ArcTool.pi", "anchor ArcTool:\n    p: {PointPrimitive.x}\n"),
        ("spec/scope/arc/Joined.pi", joined),
    ]);
    let mut first = |relative: &str| {
        let path = root.join(relative);
        let view = view_of(&mut workspace, &path);
        let file = file_of(&view, &path);
        action_titles(&view, file, &path).into_iter().next().expect("an action")
    };
    // `../point` and `/scope/point` tie, and relative wins; both beat the
    // three segments of the declaring file.
    assert_eq!(first("spec/scope/arc/ArcTool.pi").0, "Import `PointPrimitive` from `../point`");
    // A file is not imported through the index of its own directory.
    assert_eq!(first("spec/scope/point/Line.pi").0, "Import `PointPrimitive` from `./Point`");
    let (title, edit) = first("spec/scope/arc/Joined.pi");
    assert_eq!(title, "Add `PointPrimitive` to the import from `../point`");
    let changed = applied(edit);
    assert!(
        changed[&root.join("spec/scope/arc/Joined.pi")].starts_with("from ../point import Extra, PointPrimitive\n"),
        "{changed:?}"
    );
}

// ---- completion -----------------------------------------------------------------------

#[test]
fn completion_imports_what_it_offers_and_offers_what_a_holder_declares() {
    let parts = "\
export anchor Gear:
    x: 1

export anchor Spring:
    x: 1

export abstract anchor Part as part:
    q:: string
";
    let (root, mut workspace) = project(&[
        ("parts.pi", parts),
        ("a.pi", "anchor A:\n    p: {Gea}\n"),
        ("b.pi", "abstract anchor Shape:\n    name:: string\n    size:: number\n\nanchor B:\n    p: {Shape.}\n"),
        ("c.pi", "from ./parts import Gear, \n"),
        ("d.pi", "\n"),
    ]);
    let mut items = |relative: &str, needle: &str, skip: u32| {
        let path = root.join(relative);
        let view = view_of(&mut workspace, &path);
        let file = file_of(&view, &path);
        completion::complete(&view, file, offset(&view, file, needle) + TextSize::new(skip))
    };

    let offered = items("a.pi", "Gea}", 3);
    let gear = offered.iter().find(|it| it.label == "Gear").expect("an importable name is offered");
    let import = &gear.additional_text_edits.as_ref().expect("the import comes with it")[0];
    assert_eq!(import.new_text, "from ./parts import Gear\n\n");

    let offered = items("b.pi", "Shape.}", 6);
    let size = offered.iter().find(|it| it.label == "size").expect("an abstract anchor has members");
    assert_eq!(size.detail.as_deref(), Some("number"));
    assert!(offered.iter().any(|it| it.label == "name"));

    let offered = items("c.pi", "\n", 0);
    let labels: Vec<&str> = offered.iter().map(|it| it.label.as_str()).collect();
    assert!(labels.contains(&"Spring") && !labels.contains(&"Gear"), "{labels:?}");

    let offered = items("d.pi", "\n", 0);
    let part = offered.iter().find(|it| it.label == "part").expect("a keyword from another module");
    let used = &part.additional_text_edits.as_ref().expect("its `use` comes with it")[0];
    assert!(used.new_text.starts_with("use ./parts\n"), "{:?}", used.new_text);
}

// ---- presentation --------------------------------------------------------------------------

#[test]
fn a_name_is_coloured_by_what_it_resolves_to() {
    let source = "\
anchor Base:
    name: base

count: 3

anchor User:
    a: {Base}
    b: {Base.name}
    c: {count}
";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let file = file_of(&view, &path);
    let lines: Vec<&str> = source.lines().collect();
    let (mut at_line, mut at_start) = (0u32, 0u32);
    let mut produced: Vec<(u32, String, String)> = Vec::new();
    for token in tokens::semantic_tokens(&view, file) {
        at_line += token.delta_line;
        at_start = if token.delta_line == 0 { at_start + token.delta_start } else { token.delta_start };
        let text: String =
            lines[at_line as usize].chars().skip(at_start as usize).take(token.length as usize).collect();
        produced.push((at_line, text, tokens::TOKEN_TYPES[token.token_type as usize].as_str().to_string()));
    }
    let kind = |line: u32, text: &str| {
        produced
            .iter()
            .find(|(at, source, _)| *at == line && source == text)
            .map(|(_, _, kind)| kind.clone())
            .unwrap_or_else(|| panic!("no {text:?} on line {line}"))
    };
    assert_eq!(kind(6, "Base"), "class");
    assert_eq!(kind(7, "name"), "property");
    assert_eq!(kind(8, "count"), "variable");
}

#[test]
fn the_outline_says_what_each_anchor_extends_and_what_each_key_constrains() {
    let source = "\
anchor Base:
    x:: number: 1

anchor Child extends Base:
    x: 2

abstract anchor Shape as shape:
    n:: string

shape Square:
    n: s
";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let file = file_of(&view, &path);
    let symbols = tokens::document_symbols(&view, file);
    let detail = |name: &str| symbols.iter().find(|it| it.name == name).and_then(|it| it.detail.clone());
    assert_eq!(detail("Base"), None);
    assert_eq!(detail("Child").as_deref(), Some("extends Base"));
    assert_eq!(detail("Shape").as_deref(), Some("as shape"));
    assert_eq!(detail("Square").as_deref(), Some("shape"));
    let base = symbols.iter().find(|it| it.name == "Base").unwrap();
    assert_eq!(base.children.as_ref().unwrap()[0].detail.as_deref(), Some(":: number"));
}

#[test]
fn implementing_missing_properties_starts_a_body_on_its_own_line() {
    let source = "abstract anchor Shape as shape:\n    name:: string\n    count:: number\n\nshape Square:";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let file = file_of(&view, &path);
    let (_, edit) = action_titles(&view, file, &path)
        .into_iter()
        .find(|(title, _)| title == "Implement 2 missing properties")
        .expect("offered");
    assert_eq!(applied(edit)[&path], format!("{source}\n    name: TODO\n    count: 0\n"));
}

// ---- the rest of the rules --------------------------------------------------------

#[test]
fn a_move_that_takes_a_file_out_of_its_style_s_reach_leaves_the_import_alone() {
    let (root, mut workspace) = project(&[
        ("piton.config.pi", "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n"),
        ("spec/main.pi", "from /lib/Tool import Tool\n\na: {Tool}\n"),
        ("spec/lib/Tool.pi", "export anchor Tool:\n    x: 1\n"),
    ]);
    let snapshot = workspace.snapshot();
    let moves = [refactor::Move { from: root.join("spec/lib/Tool.pi"), to: root.join("outside/Tool.pi") }];
    let changed = applied(refactor::move_edits(&snapshot, &moves));
    assert!(!changed.contains_key(&root.join("spec/main.pi")), "a rooted import cannot reach outside the root: {changed:?}");
}

#[test]
fn renaming_a_keyword_renames_every_declaration_written_with_it() {
    let source = "abstract anchor Shape as shape:\n    name:: string\n\nshape Square:\n    name: s\n\nshape Circle:\n    name: c\n";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");
    let renamed = rename(&mut workspace, &path, "shape:", "form").unwrap();
    assert_eq!(renamed[&path], source.replace("as shape", "as form").replace("shape Square", "form Square").replace("shape Circle", "form Circle"));
}

#[test]
fn the_references_to_a_file_are_the_specifiers_that_load_it() {
    let (root, mut workspace) = project(&[
        ("tools.pi", "export abstract anchor Tool as tool:\n    x:: string\n"),
        ("a.pi", "from ./tools import Tool\n\nb: {Tool}\n"),
        ("b.pi", "use ./tools\n\ntool Hammer:\n    x: y\n"),
    ]);
    let path = root.join("a.pi");
    let view = view_of(&mut workspace, &path);
    let a = file_of(&view, &path);
    let module = sym(&locate(&view, a, "./tools"));
    let mut files: Vec<String> = navigation::references(&view, &module, false)
        .into_iter()
        .map(|(file, _)| view.path_of(file).unwrap().strip_prefix(&root).unwrap().display().to_string())
        .collect();
    files.sort();
    assert_eq!(files, ["a.pi", "b.pi"]);
}

#[test]
fn a_builtin_module_s_declaration_has_no_definition_to_go_to() {
    let (root, mut workspace) = project(&[
        ("piton.config.pi", "use @piton/config\n\nexport piton-config Config:\n    root: ./\n"),
        ("main.pi", "a: 1\n"),
    ]);
    let config = root.join("piton.config.pi");
    let view = view_of(&mut workspace, &config);
    let file = file_of(&view, &config);
    // The keyword is declared in `@piton/config`, which has no file.
    let keyword = locate(&view, file, "piton-config");
    assert!(navigation::definitions(&view, file, &keyword).is_empty());
    // `root:` is a key this file writes, so it is its own definition, even
    // though the property it implements is declared in the builtin module.
    let written = locate(&view, file, "root");
    assert_eq!(navigation::definitions(&view, file, &written), [(file, written.range())]);
}

#[test]
fn a_new_use_goes_after_the_last_use_or_above_the_first_import() {
    let (root, mut workspace) = project(&[
        ("x.pi", "export anchor A:\n    y: 1\n"),
        ("imports.pi", "from ./x import A\n\nb: {A}\n"),
        ("uses.pi", "use ./x\nfrom ./x import A\n\nb: {A}\n"),
        ("empty.pi", "b: 1\n"),
    ]);
    let placed = |workspace: &mut Workspace, name: &str| {
        let path = root.join(name);
        let view = view_of(workspace, &path);
        let file = file_of(&view, &path);
        apply(view.text(file), vec![imports::use_edit(&view, file, "./k")])
    };
    assert_eq!(placed(&mut workspace, "imports.pi"), "use ./k\nfrom ./x import A\n\nb: {A}\n");
    assert_eq!(placed(&mut workspace, "uses.pi"), "use ./x\nuse ./k\nfrom ./x import A\n\nb: {A}\n");
    assert_eq!(placed(&mut workspace, "empty.pi"), "use ./k\n\nb: 1\n");
}

#[test]
fn hovering_self_this_and_super_names_the_anchors_they_read() {
    let source = "anchor Base:\n    name: base\n\nanchor Child extends Base:\n    a: ${self.name}\n    b: ${this.name}\n    c: ${super.name}\n";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let file = file_of(&view, &path);
    let text = |needle: &str| hover::hover(&view, &locate(&view, file, needle)).expect(needle);
    assert!(text("self.").contains("Written in `Child`"), "{}", text("self."));
    assert!(text("this.").contains("`Child`, exactly"), "{}", text("this."));
    assert!(text("super.").contains("The bases of `Child`: `Base`"), "{}", text("super."));
}

#[test]
fn a_key_under_a_variable_shows_its_compiled_value() {
    let source = "config:\n    inner:\n        width: 2\n\na: {config.inner.width}\n";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let file = file_of(&view, &path);
    let text = hover::hover(&view, &locate(&view, file, "width}")).unwrap();
    assert!(text.contains("config.inner.width") && text.contains("\n2\n"), "{text}");
}

#[test]
fn a_missing_base_or_type_is_imported_once_by_its_quick_fix() {
    let (root, mut workspace) = project(&[
        ("shapes.pi", "export anchor Base:\n    x: 1\n"),
        ("main.pi", "anchor Child extends Base:\n    x: 2\n\nanchor Holder:\n    b:: Base: {Child}\n"),
    ]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let file = file_of(&view, &path);
    let titles: Vec<String> = action_titles(&view, file, &path).into_iter().map(|(title, _)| title).collect();
    let imports: Vec<&String> = titles.iter().filter(|it| it.starts_with("Import `Base`")).collect();
    assert_eq!(imports, [&"Import `Base` from `./shapes`".to_string()], "{titles:?}");
}

#[test]
fn an_unused_import_has_a_quick_fix_and_a_source_action() {
    let (root, mut workspace) = project(&[
        ("parts.pi", "export anchor Gear:\n    x: 1\n\nexport anchor Idle:\n    x: 1\n"),
        ("main.pi", "from ./parts import Gear, Idle\n\na: {Gear}\n"),
    ]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let file = file_of(&view, &path);
    let actions = action_titles(&view, file, &path);
    for title in ["Remove unused import `Idle`", "Remove unused imports", "Organize imports"] {
        let (_, edit) = actions.iter().find(|(it, _)| it == title).unwrap_or_else(|| panic!("{title}"));
        assert_eq!(applied(edit.clone())[&path], "from ./parts import Gear\n\na: {Gear}\n", "{title}");
    }
}

#[test]
fn completion_imports_an_anchor_after_extends_and_in_a_constraint() {
    let (root, mut workspace) = project(&[
        ("parts.pi", "export anchor Gear:\n    x: 1\n\nexport count: 3\n"),
        ("a.pi", "anchor A extends Gea:\n    x: 1\n"),
        ("b.pi", "abstract anchor B:\n    g:: Gea\n"),
    ]);
    for (name, needle) in [("a.pi", "Gea:"), ("b.pi", "Gea\n")] {
        let path = root.join(name);
        let view = view_of(&mut workspace, &path);
        let file = file_of(&view, &path);
        let offered = completion::complete(&view, file, offset(&view, file, needle) + TextSize::new(3));
        let gear = offered.iter().find(|it| it.label == "Gear").unwrap_or_else(|| panic!("{name}"));
        assert!(gear.additional_text_edits.is_some(), "{name}");
        assert!(!offered.iter().any(|it| it.label == "count"), "only anchors after extends or `::`: {name}");
    }
}

#[test]
fn accepting_a_completion_replaces_the_whole_name_being_typed() {
    let source = "use ./ui\n\nexport ui-comp";
    let (root, mut workspace) = project(&[
        ("ui.pi", "export abstract anchor UiComponent as ui-component:\n    x:: string\n"),
        ("main.pi", source),
    ]);
    let path = root.join("main.pi");
    let view = view_of(&mut workspace, &path);
    let file = file_of(&view, &path);
    let offered = completion::complete(&view, file, TextSize::new(source.len() as u32));
    let keyword = offered.iter().find(|it| it.label == "ui-component").expect("the keyword is offered");
    let Some(tower_lsp::lsp_types::CompletionTextEdit::Edit(edit)) = keyword.text_edit.clone() else {
        panic!("the item says what it replaces: {keyword:?}");
    };
    let written = apply(source, vec![edit]);
    assert!(written.starts_with("use ./ui\n\nexport ui-component ${1:Name}:"), "{written:?}");
    assert!(!written.contains("ui-ui"), "{written:?}");
}

#[test]
fn renaming_leaves_an_escape_group_alone() {
    let source = "\
anchor Tool:
    x: 1

anchor User:
    live: ${Tool} is a reference
    shown: \\ ${Tool} is not, it is text \\
";
    let (root, mut workspace) = project(&[("main.pi", source)]);
    let path = root.join("main.pi");
    let edited = rename(&mut workspace, &path, "Tool:", "Gadget").expect("renames");
    let after = edited.get(&path).expect("main.pi changed");
    assert!(after.contains("anchor Gadget:"), "{after}");
    assert!(after.contains("live: ${Gadget} is a reference"), "{after}");
    assert!(
        after.contains("shown: \\ ${Tool} is not, it is text \\"),
        "what a group holds is text, so a rename must not touch it:\n{after}"
    );
}
