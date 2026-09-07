//! Feature-level tests for the language server, driven through a real
//! workspace on disk so that resolution and evaluation are exercised too.

use std::path::PathBuf;

use piton_core::framework::Frameworks;
use tower_lsp::lsp_types::{Position, Url};

use crate::world::Workspace;
use crate::{actions, completion, hover, navigation, tokens};

/// Write a throwaway project and analyse it.
fn workspace(files: &[(&str, &str)]) -> (PathBuf, Workspace) {
    let root = std::env::temp_dir().join(format!(
        "piton-lsp-test-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    for (path, contents) in files {
        let target = root.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, contents).unwrap();
    }
    let mut workspace = Workspace::new(Frameworks::default as fn() -> Frameworks);
    workspace.set_roots(vec![root.clone()]);
    (root, workspace)
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
    let snapshot = workspace.snapshot();
    let file = snapshot.file_for(&root.join("main.pi")).expect("main is analysed");
    let messages: Vec<&str> = snapshot
        .compilation
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.file == file)
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();
    assert!(messages.iter().any(|it| it.contains("is not exported")), "{messages:?}");
    assert!(messages.iter().any(|it| it.contains("cannot find `nope`")), "{messages:?}");
}

#[test]
fn definition_and_references_cross_files() {
    let (root, mut workspace) = workspace(&[("shapes.pi", SHAPES), ("main.pi", MAIN)]);
    let snapshot = workspace.snapshot();
    let main = snapshot.file_for(&root.join("main.pi")).unwrap();
    let shapes = snapshot.file_for(&root.join("shapes.pi")).unwrap();

    let at = offset_of(MAIN, "Base\n\nanchor") ;
    let resolved = navigation::resolve(&snapshot, main, at).expect("resolves `Base`");
    let (file, range) = navigation::definition(&snapshot, &resolved.target).expect("has a home");
    assert_eq!(file, shapes);
    assert_eq!(&snapshot.text(shapes)[range], "Base");

    let references = navigation::references(&snapshot, &resolved.target);
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
    let snapshot = workspace.snapshot();
    let file = snapshot.file_for(&root.join("main.pi")).unwrap();
    let at = offset_of(source, "Shape as");
    let resolved = navigation::resolve(&snapshot, file, at).unwrap();
    assert_eq!(navigation::implementations(&snapshot, &resolved.target).len(), 2);
}

#[test]
fn hover_explains_anchors_properties_and_types() {
    let (root, mut workspace) = workspace(&[("shapes.pi", SHAPES), ("main.pi", MAIN)]);
    let snapshot = workspace.snapshot();
    let main = snapshot.file_for(&root.join("main.pi")).unwrap();

    let anchor = navigation::resolve(&snapshot, main, offset_of(MAIN, "Child")).unwrap();
    let text = hover::hover(&snapshot, &anchor).unwrap();
    assert!(text.contains("anchor Child extends Base"), "{text}");
    assert!(text.contains("summary"), "{text}");

    let variable = navigation::resolve(&snapshot, main, offset_of(MAIN, "pi:")).unwrap();
    let text = hover::hover(&snapshot, &variable).unwrap();
    assert!(text.contains("3.14"), "{text}");
}

#[test]
fn completion_knows_what_the_line_expects() {
    let source = "\
abstract anchor Shape as shape:
    description:: string

shape Concrete:
    d
";
    let (root, mut workspace) = workspace(&[("main.pi", source)]);
    let snapshot = workspace.snapshot();
    let file = snapshot.file_for(&root.join("main.pi")).unwrap();

    // Inside an anchor body: the properties it still owes.
    let inside = offset_of(source, "    d\n") + piton_syntax::TextSize::new(5);
    let labels: Vec<String> =
        completion::complete(&snapshot, file, inside).into_iter().map(|it| it.label).collect();
    assert!(labels.iter().any(|it| it == "description"), "{labels:?}");

    // At the top level: keywords, including the user-defined ones.
    let top = piton_syntax::TextSize::new(source.find("shape Concrete").unwrap() as u32);
    let labels: Vec<String> =
        completion::complete(&snapshot, file, top).into_iter().map(|it| it.label).collect();
    assert!(labels.iter().any(|it| it == "anchor"), "{labels:?}");
    assert!(labels.iter().any(|it| it == "shape"), "{labels:?}");

    // After `::`: types.
    let after = offset_of(source, "string") ;
    let labels: Vec<String> =
        completion::complete(&snapshot, file, after).into_iter().map(|it| it.label).collect();
    assert!(labels.iter().any(|it| it == "complex"), "{labels:?}");
}

#[test]
fn semantic_tokens_cover_the_document() {
    let (root, mut workspace) = workspace(&[("shapes.pi", SHAPES), ("main.pi", MAIN)]);
    let snapshot = workspace.snapshot();
    let file = snapshot.file_for(&root.join("main.pi")).unwrap();
    let produced = tokens::semantic_tokens(&snapshot, file);
    assert!(produced.len() > 10, "expected a token per meaningful word: {}", produced.len());
    assert!(produced.iter().all(|token| (token.token_type as usize) < tokens::TOKEN_TYPES.len()));
}

#[test]
fn symbols_folding_links_and_hints() {
    let (root, mut workspace) = workspace(&[("shapes.pi", SHAPES), ("main.pi", MAIN)]);
    let snapshot = workspace.snapshot();
    let file = snapshot.file_for(&root.join("main.pi")).unwrap();

    let symbols = tokens::document_symbols(&snapshot, file);
    assert!(symbols.iter().any(|symbol| symbol.name == "Child"));
    assert!(symbols.iter().any(|symbol| symbol.name == "pi"));
    let child = symbols.iter().find(|symbol| symbol.name == "Child").unwrap();
    assert!(child.children.as_ref().is_some_and(|it| it.iter().any(|c| c.name == "detail")));

    assert!(!tokens::folding_ranges(&snapshot, file).is_empty());

    let links = tokens::document_links(&snapshot, file);
    assert_eq!(links.len(), 1, "the import path should be a link");

    let hints = tokens::inlay_hints(&snapshot, file);
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
    let snapshot = workspace.snapshot();
    let file = snapshot.file_for(&root.join("main.pi")).unwrap();
    let url = Url::from_file_path(root.join("main.pi")).unwrap();
    let range = piton_syntax::TextRange::new(
        offset_of(source, "shape Concrete"),
        offset_of(source, "shape Concrete") + piton_syntax::TextSize::new(14),
    );
    let offered = actions::actions(&snapshot, file, &url, range);
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
    let snapshot = workspace.snapshot();
    let file = snapshot.file_for(&root.join("main.pi")).unwrap();
    let index = snapshot.line_index(file);
    let at = index.offset(Position { line: 0, character: 20 });
    assert_eq!(&source[usize::from(at)..usize::from(at) + 1], "x");
    assert_eq!(index.position(at), Position { line: 0, character: 20 });
}
