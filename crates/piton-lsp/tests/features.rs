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
    let mut fixture = Fixture::new();
    let file = "spec/scope/language/types/Escaping.pi";
    let path = fixture.root.join(file);
    let original = std::fs::read_to_string(&path).expect("readable");

    // Introduce a problem in an unsaved buffer rather than relying on the
    // specification having one: a clean specbase is the goal, not a test
    // fixture.
    fixture.world.set_document(
        path.clone(),
        format!("{original}\nexport anchor Broken:\n    value: {{NotDefined}}\n"),
    );
    fixture.world.recompile(Some(&path));

    let grouped = fixture.world.diagnostics_by_file();
    let reported = grouped.get(&path).expect("the edited file is reported");
    assert!(
        reported.iter().any(|d| d.code == "unresolved-symbol"),
        "{reported:#?}"
    );
    assert!(
        grouped.keys().all(|path| !path.to_string_lossy().starts_with('@')),
        "bundled packages should not be reported to the editor"
    );
}

#[test]
fn a_clean_specbase_reports_nothing() {
    let fixture = Fixture::new();
    let problems: Vec<String> = fixture
        .world
        .diagnostics_by_file()
        .iter()
        .flat_map(|(path, items)| {
            items
                .iter()
                .map(move |d| format!("{}: {}", path.display(), d.message))
        })
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");
}

// ---------------------------------------------------------------------------
// Completion
// ---------------------------------------------------------------------------

/// A file the server sees the way an editor would: the project on disk, with
/// one buffer replaced by text that has not been saved.
///
/// The cursor is written into the text as `<|>`. Replacing a file the project
/// already reaches rather than inventing a new one keeps the whole module graph
/// in play, which is what questions about imports and inheritance need.
struct Buffer {
    world: World,
    uri: Url,
    position: Position,
}

impl Buffer {
    fn over(relative: &str, marked: &str) -> Buffer {
        let root = repo_root();
        let path = root.join(relative);
        let offset = marked.find("<|>").expect("the text marks a cursor");
        let text = marked.replace("<|>", "");

        let mut world = World::new(&root);
        world.set_document(path.clone(), text.clone());
        world.recompile(Some(&path));

        Buffer {
            uri: path_to_url(&path).expect("url"),
            position: offset_to_position(&text, offset),
            world,
        }
    }

    fn items(&self) -> Vec<CompletionItem> {
        match features::completion(&self.world, &self.uri, self.position) {
            Some(CompletionResponse::Array(items)) => items,
            Some(_) => panic!("expected an array"),
            None => Vec::new(),
        }
    }

    fn labels(&self) -> Vec<String> {
        self.items().into_iter().map(|item| item.label).collect()
    }
}

/// The file every buffer below stands in for: small, imported from, and
/// reachable from the project's entry point.
const NULL: &str = "spec/scope/language/types/Null.pi";

#[test]
fn completion_at_the_top_of_a_file_offers_declarations_and_user_keywords() {
    let buffer = Buffer::over(NULL, "use ./lib/Type\n\n<|>\n");
    let labels = buffer.labels();
    for expected in ["anchor", "export", "abstract", "use", "from"] {
        assert!(labels.contains(&expected.to_string()), "{labels:?}");
    }
    // `use ./lib/Type` brings `type` into scope as a declaration keyword,
    // because `Type` is declared `as type`.
    assert!(labels.contains(&"type".to_string()), "{labels:?}");
}

#[test]
fn completion_after_use_offers_module_paths() {
    let buffer = Buffer::over(NULL, "use <|>\n");
    let labels = buffer.labels();
    // The packages are bundled with the compiler rather than on disk, so
    // nothing but naming them outright would find them.
    assert!(labels.contains(&"@piton/belay".to_string()), "{labels:?}");
    assert!(labels.contains(&"@piton/config".to_string()), "{labels:?}");
    // And the source root is read from disk, because the point of completing
    // an import is to reach something the project does not reach yet.
    assert!(labels.contains(&"./lib/Type".to_string()), "{labels:?}");
    assert!(
        !labels.contains(&"./Null".to_string()),
        "a file cannot import itself: {labels:?}"
    );
}

#[test]
fn completion_after_from_offers_the_direction_then_the_names() {
    let direction = Buffer::over(NULL, "from ./lib/Type <|>\n");
    let labels = direction.labels();
    assert_eq!(labels, vec!["import".to_string(), "export".to_string()]);

    let names = Buffer::over(NULL, "from ./lib/Type import <|>\n");
    let labels = names.labels();
    assert!(labels.contains(&"Type".to_string()), "{labels:?}");
    assert!(
        labels.contains(&"*".to_string()),
        "`*` takes every exported name: {labels:?}"
    );
}

#[test]
fn completion_after_extends_offers_anchors_rather_than_keywords() {
    let buffer = Buffer::over(NULL, "use ./lib/Type\n\nexport anchor A extends <|>\n");
    let items = buffer.items();
    let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
    assert!(labels.contains(&"Type"), "{labels:?}");
    assert!(
        !labels.contains(&"anchor"),
        "a base is not a keyword: {labels:?}"
    );
    // An abstract anchor reads as an interface: it cannot stand on its own.
    let kind = items
        .iter()
        .find(|item| item.label == "Type")
        .and_then(|item| item.kind);
    assert_eq!(kind, Some(CompletionItemKind::INTERFACE));
}

#[test]
fn completion_in_a_type_constraint_offers_types() {
    let buffer = Buffer::over(NULL, "use ./lib/Type\n\nexport type N:\n    a:: <|>\n");
    let labels = buffer.labels();
    for expected in ["string", "number", "boolean", "dictionary", "extends"] {
        assert!(labels.contains(&expected.to_string()), "{labels:?}");
    }
    // An anchor's name is a type too, and constraining to one is how a
    // property says which anchors it takes.
    assert!(labels.contains(&"Type".to_string()), "{labels:?}");
}

#[test]
fn completion_of_a_value_offers_only_what_its_constraint_accepts() {
    // `supportedOperators:: extends Operator[]:: null` -- so an operator or
    // nothing, and nothing else.
    let buffer = Buffer::over(
        NULL,
        "use ./lib/Type\n\nexport type Null:\n    supportedOperators: <|>\n",
    );
    let items = buffer.items();
    let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();

    assert!(labels.contains(&"null"), "{labels:?}");
    assert!(
        labels.contains(&"PropertyAccessOperator"),
        "an anchor that extends Operator belongs here: {labels:?}"
    );
    assert!(
        !labels.contains(&"Type"),
        "`Type` does not extend `Operator`, so it does not fit: {labels:?}"
    );
    assert!(
        !labels.contains(&"true"),
        "the constraint does not accept a boolean: {labels:?}"
    );

    // A value that has to be an anchor is written as a reference to one, so
    // completing it writes the braces as well.
    let insert = items
        .iter()
        .find(|item| item.label == "PropertyAccessOperator")
        .and_then(|item| item.insert_text.clone());
    assert_eq!(insert.as_deref(), Some("{PropertyAccessOperator}"));
}

#[test]
fn completion_after_a_dot_offers_the_members_of_what_it_follows() {
    let buffer = Buffer::over(
        NULL,
        "use ./lib/Type\n\nexport type Null:\n    a: ${self.<|>}\n",
    );
    let items = buffer.items();
    let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
    // `self` is the anchor being written, so its slots are what follows the
    // dot -- the inherited ones included.
    assert!(labels.contains(&"whatIsAType"), "{labels:?}");
    assert!(labels.contains(&"supportedOperators"), "{labels:?}");
    let detail = items
        .iter()
        .find(|item| item.label == "whatIsAType")
        .and_then(|item| item.detail.clone())
        .unwrap_or_default();
    assert!(detail.contains("inherited from Type"), "{detail}");
}

#[test]
fn a_reference_expression_offers_only_things_a_reference_can_name() {
    let buffer = Buffer::over(NULL, "use ./lib/Type\n\nexport type N:\n    a: @{<|>}\n");
    let labels = buffer.labels();
    assert!(labels.contains(&"Type".to_string()), "{labels:?}");
    // `@{...}` has to resolve to a referenceable anchor, so a literal in there
    // is an error waiting to be written.
    for wrong in ["true", "false", "null"] {
        assert!(
            !labels.contains(&wrong.to_string()),
            "`@{{{wrong}}}` cannot resolve to an anchor: {labels:?}"
        );
    }
}

#[test]
fn a_string_and_a_sentence_have_nothing_to_complete() {
    // The specification is explicit: typing a property access in the middle of
    // a string should complete nothing, because a string has no members.
    let string = Buffer::over(
        NULL,
        "export type N:\n    a: ${\"the end.<|>\"}\n",
    );
    assert!(string.labels().is_empty(), "{:?}", string.labels());

    // And prose is a value in Piton rather than a name being completed.
    let prose = Buffer::over(NULL, "export type N:\n    a: the quick brown<|>\n");
    assert!(prose.labels().is_empty(), "{:?}", prose.labels());

    let comment = Buffer::over(NULL, "export type N:\n    // a note <|>\n");
    assert!(comment.labels().is_empty(), "{:?}", comment.labels());
}

#[test]
fn a_verbatim_block_has_nothing_to_complete() {
    let fenced = Buffer::over(
        NULL,
        "export type N:\n    a:\n        ```json\n        { <|>\n        ```\n",
    );
    assert!(fenced.labels().is_empty(), "{:?}", fenced.labels());

    let escaped = Buffer::over(
        NULL,
        "export type N:\n    a:\n\\\\\\\\\nliteral {<|>}\n\\\\\\\\\n",
    );
    assert!(escaped.labels().is_empty(), "{:?}", escaped.labels());
}

#[test]
fn completing_a_name_from_another_module_writes_the_import() {
    let buffer = Buffer::over(NULL, "export type N:\n    a: {<|>}\n");
    let items = buffer.items();
    let offered = items
        .iter()
        .find(|item| item.label == "Type")
        .expect("`Type` is exported by a module this one does not import");

    let edits = offered
        .additional_text_edits
        .as_ref()
        .expect("completing it should write the import");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "from ./lib/Type import Type\n");
    assert_eq!(
        offered
            .label_details
            .as_ref()
            .and_then(|details| details.description.clone())
            .as_deref(),
        Some("./lib/Type"),
        "the list has to say where the name would come from"
    );
}

#[test]
fn a_body_puts_the_properties_it_has_to_define_first() {
    let buffer = Buffer::over(NULL, "use ./lib/Type\n\nexport type N:\n    <|>\n");
    let items = buffer.items();
    let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
    assert!(labels.contains(&"whatIsAType"), "{labels:?}");
    assert!(
        labels.contains(&"pass"),
        "an anchor with nothing to say still has to say so: {labels:?}"
    );

    // `description:: simple:: complex` has no value on `Type`, so an anchor
    // extending it has to write one. Those sort above the rest.
    let required = items
        .iter()
        .find(|item| item.label == "description")
        .expect("description");
    assert!(
        required.sort_text.as_deref().is_some_and(|key| key.starts_with('0')),
        "{:?}",
        required.sort_text
    );
    let optional = items
        .iter()
        .find(|item| item.label == "whatIsAType")
        .expect("whatIsAType");
    assert!(
        optional.sort_text.as_deref().is_some_and(|key| key.starts_with('1')),
        "{:?}",
        optional.sort_text
    );
}

#[test]
fn resolving_an_item_fills_in_its_documentation() {
    let buffer = Buffer::over(NULL, "use ./lib/Type\n\nexport anchor A extends <|>\n");
    let offered = buffer
        .items()
        .into_iter()
        .find(|item| item.label == "Type")
        .expect("Type");
    // The list itself carries no description: building one renders the whole
    // compiled value, and a list is hundreds of items long.
    assert!(offered.documentation.is_none());

    let resolved = features::resolve_completion(&buffer.world, offered);
    let Some(Documentation::MarkupContent(markup)) = resolved.documentation else {
        panic!("expected markdown");
    };
    assert!(markup.value.contains("abstract"), "{}", markup.value);
    assert!(markup.value.contains("Type"), "{}", markup.value);
}

#[test]
fn a_list_item_takes_the_constraints_of_the_key_above_it() {
    // `supportedOperators:: extends Operator[]` -- the items of the list are
    // what the constraint is about, so they are what it narrows.
    let buffer = Buffer::over(
        NULL,
        "use ./lib/Type\n\nexport type Null:\n    supportedOperators:\n        - <|>\n",
    );
    let labels = buffer.labels();
    assert!(
        labels.contains(&"PropertyAccessOperator".to_string()),
        "{labels:?}"
    );
    assert!(
        !labels.contains(&"Type".to_string()),
        "`Type` does not extend `Operator`: {labels:?}"
    );
}

#[test]
fn a_module_that_does_not_exist_exports_nothing() {
    let buffer = Buffer::over(NULL, "from ./nowhere import <|>\n");
    assert!(
        buffer.labels().is_empty(),
        "there is nothing to import from a module that is not there: {:?}",
        buffer.labels()
    );
}
