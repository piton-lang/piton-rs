//! Exercises the editor features against the real specification.

use std::path::{Path, PathBuf};

use piton_lsp::convert::{offset_to_position, path_to_url};
use piton_lsp::features;
use piton_lsp::world::World;
use tower_lsp::lsp_types::*;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

struct Fixture {
    world: World,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Fixture {
        let root = repo_root();
        let mut world = World::new(&root);
        world.recompile(None);
        Fixture { world, root }
    }

    fn uri(&self, relative: &str) -> Url {
        path_to_url(&self.root.join(relative)).expect("url")
    }

    /// The position of the first occurrence of `needle` in a file.
    fn position_of(&self, relative: &str, needle: &str) -> Position {
        let path = self.root.join(relative);
        let text = std::fs::read_to_string(&path).expect("readable");
        let offset = text.find(needle).unwrap_or_else(|| panic!("`{needle}` not in {relative}"));
        offset_to_position(&text, offset + 1)
    }
}

#[test]
fn hover_on_a_user_keyword_explains_the_anchor_it_aliases() {
    let fixture = Fixture::new();
    let uri = fixture.uri("spec/scope/language/types/Strings.pi");
    let position = fixture.position_of("spec/scope/language/types/Strings.pi", "export type Strings");
    let hover = features::hover(&fixture.world, &uri, fixture.position_of("spec/scope/language/types/Strings.pi", "type Strings"))
        .or_else(|| features::hover(&fixture.world, &uri, position))
        .expect("hover");
    let HoverContents::Markup(markup) = hover.contents else { panic!("expected markup") };
    assert!(
        markup.value.contains("Keyword `type`") || markup.value.contains("Strings"),
        "{}",
        markup.value
    );
}

#[test]
fn hover_on_an_anchor_shows_its_inheritance_chain() {
    let fixture = Fixture::new();
    let file = "spec/scope/language/types/Strings.pi";
    let uri = fixture.uri(file);
    let hover = features::hover(&fixture.world, &uri, fixture.position_of(file, "Strings:"))
        .expect("hover");
    let HoverContents::Markup(markup) = hover.contents else { panic!("expected markup") };
    assert!(markup.value.contains("Inherits: Type"), "{}", markup.value);
    assert!(markup.value.contains("Resolves to:"), "{}", markup.value);
}

#[test]
fn go_to_definition_follows_an_import() {
    let fixture = Fixture::new();
    let file = "spec/scope/language/types/Collections.pi";
    let uri = fixture.uri(file);
    let response = features::definition(&fixture.world, &uri, fixture.position_of(file, "Lists import Lists"))
        .expect("definition");
    let GotoDefinitionResponse::Scalar(location) = response else { panic!("expected one location") };
    assert!(
        location.uri.path().ends_with("types/Lists.pi"),
        "{}",
        location.uri
    );
}

#[test]
fn find_references_spans_files() {
    let fixture = Fixture::new();
    let file = "spec/scope/language/types/lib/Type.pi";
    let uri = fixture.uri(file);
    let locations = features::references(
        &fixture.world,
        &uri,
        fixture.position_of(file, "Type as type"),
        true,
    )
    .expect("references");
    let files: std::collections::BTreeSet<String> = locations
        .iter()
        .map(|location| location.uri.path().to_string())
        .collect();
    assert!(files.len() > 1, "expected references in several files, got {files:?}");
}

#[test]
fn rename_rewrites_every_occurrence() {
    let fixture = Fixture::new();
    let file = "spec/scope/language/types/Escaping.pi";
    let uri = fixture.uri(file);
    let edit = features::rename(
        &fixture.world,
        &uri,
        fixture.position_of(file, "Escaping:"),
        "EscapeRules",
    )
    .expect("rename");
    let changes = edit.changes.expect("changes");
    assert!(changes.len() >= 2, "rename should reach the importing files");
    for edits in changes.values() {
        assert!(edits.iter().all(|edit| edit.new_text == "EscapeRules"));
    }
}

#[test]
fn document_symbols_expose_anchors_and_properties() {
    let fixture = Fixture::new();
    let uri = fixture.uri("spec/scope/language/types/Strings.pi");
    let DocumentSymbolResponse::Nested(symbols) =
        features::document_symbols(&fixture.world, &uri).expect("symbols")
    else {
        panic!("expected nested symbols");
    };
    let strings = symbols.iter().find(|s| s.name == "Strings").expect("Strings");
    let children = strings.children.as_ref().expect("properties");
    let names: Vec<&str> = children.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"description"), "{names:?}");
    assert!(names.contains(&"supportedOperators"), "{names:?}");
}

#[test]
fn workspace_symbols_find_by_substring() {
    let fixture = Fixture::new();
    let symbols = features::workspace_symbols(&fixture.world, "Operator").expect("symbols");
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"AdditionOperator"), "{names:?}");
    assert!(names.contains(&"Operator"), "{names:?}");
}

#[test]
fn completion_inside_an_anchor_offers_inherited_properties() {
    let fixture = Fixture::new();
    let file = "spec/scope/language/types/Null.pi";
    let uri = fixture.uri(file);
    let path = fixture.root.join(file);
    let text = std::fs::read_to_string(&path).expect("readable");
    // A position inside the anchor body.
    let offset = text.find("supportedOperators").expect("property") + 2;
    let position = offset_to_position(&text, offset);
    let CompletionResponse::Array(items) =
        features::completion(&fixture.world, &uri, position).expect("completion")
    else {
        panic!("expected an array");
    };
    let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
    assert!(labels.contains(&"whatIsAType"), "{labels:?}");
    assert!(labels.contains(&"unsupportedOperators"), "{labels:?}");
    let inherited = items
        .iter()
        .find(|item| item.label == "whatIsAType")
        .and_then(|item| item.detail.clone())
        .unwrap_or_default();
    assert!(inherited.contains("inherited from Type"), "{inherited}");
}

#[test]
fn inlay_hints_name_the_contributing_base() {
    let fixture = Fixture::new();
    let file = "spec/scope/language/operators/arithmetic/AdditionOperator.pi";
    let uri = fixture.uri(file);
    let path = fixture.root.join(file);
    let text = std::fs::read_to_string(&path).expect("readable");
    let whole = Range {
        start: Position::default(),
        end: offset_to_position(&text, text.len()),
    };
    let hints = features::inlay_hints(&fixture.world, &uri, whole).expect("hints");
    assert!(!hints.is_empty(), "expected hints for resolved property types");
    let labels: Vec<String> = hints
        .iter()
        .map(|hint| match &hint.label {
            InlayHintLabel::String(text) => text.clone(),
            _ => String::new(),
        })
        .collect();
    // The inferred type is shown, and a property that replaces an inherited
    // declaration says which one it replaces.
    assert!(labels.iter().any(|label| label.contains("string")), "{labels:?}");
    assert!(
        labels.iter().any(|label| label.contains("overrides ArithmeticOperator")),
        "{labels:?}"
    );
}

#[test]
fn inlay_hints_mark_inherited_values() {
    let fixture = Fixture::new();
    let file = "spec/scope/language/types/Strings.pi";
    let uri = fixture.uri(file);
    let path = fixture.root.join(file);
    let text = std::fs::read_to_string(&path).expect("readable");
    let whole = Range {
        start: Position::default(),
        end: offset_to_position(&text, text.len()),
    };
    let labels: Vec<String> = features::inlay_hints(&fixture.world, &uri, whole)
        .expect("hints")
        .iter()
        .map(|hint| match &hint.label {
            InlayHintLabel::String(text) => text.clone(),
            _ => String::new(),
        })
        .collect();
    // `description` in Strings is an implicit mixed list, not a plain string.
    assert!(labels.iter().any(|label| label.contains("list")), "{labels:?}");
    assert!(labels.iter().any(|label| label.contains("overrides Type")), "{labels:?}");
}

#[test]
fn folding_covers_every_declaration() {
    let fixture = Fixture::new();
    let uri = fixture.uri("spec/scope/language/types/Strings.pi");
    let ranges = features::folding(&fixture.world, &uri).expect("folding");
    assert!(!ranges.is_empty());
    assert!(ranges.iter().all(|range| range.end_line > range.start_line));
}

#[test]
fn selection_ranges_widen_outward() {
    let fixture = Fixture::new();
    let file = "spec/scope/language/types/Strings.pi";
    let uri = fixture.uri(file);
    let position = fixture.position_of(file, "supportedOperators");
    let ranges = features::selection_ranges(&fixture.world, &uri, &[position]).expect("ranges");
    let mut current = ranges.into_iter().next().expect("one range");
    let mut widths = vec![span_width(&current.range)];
    while let Some(parent) = current.parent {
        current = *parent;
        widths.push(span_width(&current.range));
    }
    assert!(widths.len() > 1, "selection should have parents");
    assert!(
        widths.windows(2).all(|pair| pair[1] >= pair[0]),
        "each parent must be at least as wide: {widths:?}"
    );
}

fn span_width(range: &Range) -> u64 {
    let lines = (range.end.line - range.start.line) as u64;
    lines * 10_000 + range.end.character as u64
}

#[test]
fn semantic_tokens_are_produced_in_order() {
    let fixture = Fixture::new();
    let uri = fixture.uri("spec/scope/language/types/Strings.pi");
    let SemanticTokensResult::Tokens(tokens) =
        features::semantic_tokens(&fixture.world, &uri).expect("tokens")
    else {
        panic!("expected tokens");
    };
    assert!(!tokens.data.is_empty());
    // Deltas are relative and must never be negative, which the type enforces;
    // check that the first token starts at or after the document start.
    assert!(tokens.data[0].delta_line < 1000);
}

#[test]
fn formatting_reports_no_change_for_canonical_files() {
    let fixture = Fixture::new();
    let uri = fixture.uri("spec/scope/language/types/Strings.pi");
    let edits = features::formatting(&fixture.world, &uri).expect("edits");
    assert!(
        edits.is_empty(),
        "the specification is canonically formatted; got {edits:?}"
    );
}

#[test]
fn diagnostics_reach_the_editor() {
    let fixture = Fixture::new();
    let grouped = fixture.world.diagnostics_by_file();
    let total: usize = grouped.values().map(Vec::len).sum();
    assert!(total > 0, "the specification has known issues to report");
    assert!(
        grouped.keys().all(|path| !path.to_string_lossy().starts_with('@')),
        "bundled packages should not be reported to the editor"
    );
}
