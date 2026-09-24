//! Exercises the editor features against the real specification.

use std::path::{Path, PathBuf};

use piton_lsp::convert::{offset_to_position, path_to_url, position_to_offset};
use piton_lsp::features;
use piton_lsp::world::World;
use piton_lsp::TOKEN_TYPES;
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
        let offset = text
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` not in {relative}"));
        offset_to_position(&text, offset + 1)
    }
}

#[test]
fn hover_on_a_user_keyword_explains_the_anchor_it_aliases() {
    let fixture = Fixture::new();
    let uri = fixture.uri("spec/scope/language/types/Strings.pi");
    let position = fixture.position_of(
        "spec/scope/language/types/Strings.pi",
        "export type Strings",
    );
    let hover = features::hover(
        &fixture.world,
        &uri,
        fixture.position_of("spec/scope/language/types/Strings.pi", "type Strings"),
    )
    .or_else(|| features::hover(&fixture.world, &uri, position))
    .expect("hover");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markup")
    };
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
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markup")
    };
    assert!(markup.value.contains("Inherits: Type"), "{}", markup.value);
    // The compiled interpretation is shown in the project's renderer (JSON by
    // default).
    assert!(markup.value.contains("Resolves to (json):"), "{}", markup.value);
    assert!(markup.value.contains("```json"), "{}", markup.value);
}

#[test]
fn go_to_definition_follows_an_import() {
    let fixture = Fixture::new();
    let file = "spec/scope/language/types/Collections.pi";
    let uri = fixture.uri(file);
    let response = features::definition(
        &fixture.world,
        &uri,
        fixture.position_of(file, "Lists import Lists"),
    )
    .expect("definition");
    let GotoDefinitionResponse::Scalar(location) = response else {
        panic!("expected one location")
    };
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
    assert!(
        files.len() > 1,
        "expected references in several files, got {files:?}"
    );
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
    assert!(
        changes.len() >= 2,
        "rename should reach the importing files"
    );
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
    let strings = symbols
        .iter()
        .find(|s| s.name == "Strings")
        .expect("Strings");
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
    assert!(
        !hints.is_empty(),
        "expected hints for resolved property types"
    );
    let labels: Vec<String> = hints
        .iter()
        .map(|hint| match &hint.label {
            InlayHintLabel::String(text) => text.clone(),
            _ => String::new(),
        })
        .collect();
    // The inferred type is shown, and a property that replaces an inherited
    // declaration says which one it replaces.
    assert!(
        labels.iter().any(|label| label.contains("string")),
        "{labels:?}"
    );
    assert!(
        labels
            .iter()
            .any(|label| label.contains("overrides ArithmeticOperator")),
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
    assert!(
        labels.iter().any(|label| label.contains("list")),
        "{labels:?}"
    );
    assert!(
        labels.iter().any(|label| label.contains("overrides Type")),
        "{labels:?}"
    );
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
fn formatting_leaves_comments_alone() {
    // The LSP `formatting` handler is what an editor calls on save, so it is
    // the "autoformat" the spec governs: "Autoformat should add the space
    // after `//`, but not touch anything else that's commented."
    let mut fixture = Fixture::new();
    let path = fixture.root.join("spec/scope/language/types/Strings.pi");
    let original = std::fs::read_to_string(&path).expect("readable");

    fixture.world.set_document(
        path.clone(),
        format!("{original}\n//no space here\n//   indented:  x\nanchor Pad:\n  value: 1\n"),
    );
    fixture.world.recompile(Some(&path));

    let uri = fixture.uri("spec/scope/language/types/Strings.pi");
    let edits = features::formatting(&fixture.world, &uri).expect("edits");
    let text = fixture.world.text(&path).expect("text");
    let mut formatted = text.clone();
    for edit in edits.iter().rev() {
        let start = piton_lsp::convert::position_to_offset(&formatted, edit.range.start);
        let end = piton_lsp::convert::position_to_offset(&formatted, edit.range.end);
        formatted.replace_range(start..end, &edit.new_text);
    }

    assert!(
        formatted.contains("// no space here"),
        "autoformat adds the space after `//`:\n{formatted}"
    );
    assert!(
        formatted.contains("//   indented:  x"),
        "and leaves the rest of the comment byte for byte:\n{formatted}"
    );
    assert!(
        formatted.contains("    value: 1"),
        "autoformat still formats the code around it:\n{formatted}"
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
        grouped
            .keys()
            .all(|path| !path.to_string_lossy().starts_with('@')),
        "bundled packages should not be reported to the editor"
    );
}

#[test]
fn a_clean_specbase_reports_nothing() {
    let fixture = Fixture::new();
    let compilation = fixture.world.compilation.as_ref().expect("compiled");
    // Editor analysis (unused imports, Belay plan checks) is allowed to notice
    // things the compiler accepts. This test holds the compiler's own
    // diagnostics: a clean specbase still compiles.
    let problems: Vec<String> = compilation
        .diagnostics
        .iter()
        .filter(|diagnostic| !diagnostic.file.to_string_lossy().starts_with('@'))
        .map(|diagnostic| format!("{}: {}", diagnostic.file.display(), diagnostic.message))
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");
    assert!(
        fixture
            .world
            .diagnostics_by_file()
            .keys()
            .all(|path| !path.to_string_lossy().starts_with('@')),
        "bundled packages should not be reported to the editor"
    );
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
    text: String,
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
            text,
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

    /// The document as it would read after accepting the item with this label.
    ///
    /// The edit takes the place of the range it covers, which is the whole
    /// question here: a completion replaces the text being typed rather than
    /// being appended to it.
    fn accept(&self, label: &str) -> String {
        let item = self
            .items()
            .into_iter()
            .find(|item| item.label == label)
            .unwrap_or_else(|| panic!("no item labeled `{label}`"));
        let edit = match item.text_edit {
            Some(CompletionTextEdit::Edit(edit)) => edit,
            other => panic!("a completion should carry a plain replacement, got {other:?}"),
        };
        let start = position_to_offset(&self.text, edit.range.start);
        let end = position_to_offset(&self.text, edit.range.end);
        let mut applied = self.text.clone();
        applied.replace_range(start..end, &edit.new_text);
        applied
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
fn accepting_a_candidate_replaces_the_text_being_written() {
    // The word under the cursor is what gets replaced: `anc` becomes
    // `anchor`, not `ancanchor`.
    let declaration = Buffer::over(NULL, "use ./lib/Type\n\nanc<|>\n");
    assert_eq!(declaration.accept("anchor"), "use ./lib/Type\n\nanchor\n");

    // A module path is written whole: the at-sign and the slashes are part of
    // what is being replaced.
    let bundled = Buffer::over(NULL, "use @piton/be<|>\n");
    assert_eq!(bundled.accept("@piton/belay"), "use @piton/belay\n");

    let relative = Buffer::over(NULL, "from ./lib/Ty<|>\n");
    assert_eq!(relative.accept("./lib/Type"), "from ./lib/Type\n");

    // Only the name being imported moves; the import around it stands.
    let imported = Buffer::over(NULL, "from ./lib/Type import Ty<|>\n");
    assert_eq!(imported.accept("Type"), "from ./lib/Type import Type\n");

    // A base takes the place of the half-written name it stands in for.
    let base = Buffer::over(NULL, "use ./lib/Type\n\nexport anchor A extends Ty<|>\n");
    assert_eq!(
        base.accept("Type"),
        "use ./lib/Type\n\nexport anchor A extends Type\n"
    );

    // A member is only ever the segment after the dot; the expression it hangs
    // off stays as it was.
    let member = Buffer::over(
        NULL,
        "use ./lib/Type\n\nexport type Null:\n    a: ${self.sup<|>}\n",
    );
    assert_eq!(
        member.accept("supportedOperators"),
        "use ./lib/Type\n\nexport type Null:\n    a: ${self.supportedOperators}\n"
    );

    // A key in a body takes its colon with it, under the indent it was
    // written at.
    let key = Buffer::over(NULL, "use ./lib/Type\n\nexport type N:\n    wh<|>\n");
    assert_eq!(
        key.accept("whatIsAType"),
        "use ./lib/Type\n\nexport type N:\n    whatIsAType: \n"
    );

    // The `::` before a constraint is not part of the type being named.
    let constraint = Buffer::over(NULL, "export type N:\n    a:: str<|>\n");
    assert_eq!(
        constraint.accept("string"),
        "export type N:\n    a:: string\n"
    );

    // The `-` that opens a list item is the item's, not the value's -- even
    // written against it with no space.
    let item = Buffer::over(
        NULL,
        "use ./lib/Type\n\nexport type Null:\n    supportedOperators:\n        - nu<|>\n",
    );
    assert_eq!(
        item.accept("null"),
        "use ./lib/Type\n\nexport type Null:\n    supportedOperators:\n        - null\n"
    );
    let glued = Buffer::over(
        NULL,
        "use ./lib/Type\n\nexport type Null:\n    supportedOperators:\n        -nu<|>\n",
    );
    assert_eq!(
        glued.accept("null"),
        "use ./lib/Type\n\nexport type Null:\n    supportedOperators:\n        -null\n"
    );

    // With nothing written yet there is nothing to replace, so the edit
    // inserts where the cursor is.
    let empty = Buffer::over(NULL, "use <|>\n");
    let offered = empty
        .items()
        .into_iter()
        .find(|item| item.label == "@piton/belay")
        .expect("@piton/belay");
    let edit = match offered.text_edit {
        Some(CompletionTextEdit::Edit(edit)) => edit,
        other => panic!("a completion should carry a plain replacement, got {other:?}"),
    };
    assert_eq!(edit.range.start, edit.range.end, "{edit:?}");
    assert_eq!(edit.new_text, "@piton/belay");
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
        "use ./lib/Type\n\nexport type Null:\n    supportedOperators: n<|>\n",
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
    let string = Buffer::over(NULL, "export type N:\n    a: ${\"the end.<|>\"}\n");
    assert!(string.labels().is_empty(), "{:?}", string.labels());

    // And prose is a value in Piton rather than a name being completed.
    let prose = Buffer::over(NULL, "export type N:\n    a: the quick brown<|>\n");
    assert!(prose.labels().is_empty(), "{:?}", prose.labels());

    let comment = Buffer::over(NULL, "export type N:\n    // a note <|>\n");
    assert!(comment.labels().is_empty(), "{:?}", comment.labels());
}

#[test]
fn a_verbatim_block_has_nothing_to_complete() {
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
        required
            .sort_text
            .as_deref()
            .is_some_and(|key| key.starts_with('0')),
        "{:?}",
        required.sort_text
    );
    let optional = items
        .iter()
        .find(|item| item.label == "whatIsAType")
        .expect("whatIsAType");
    assert!(
        optional
            .sort_text
            .as_deref()
            .is_some_and(|key| key.starts_with('1')),
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
        "use ./lib/Type\n\nexport type Null:\n    supportedOperators:\n        - P<|>\n",
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

// ---------------------------------------------------------------------------
// On-type formatting
// ---------------------------------------------------------------------------

/// The edits pressing enter produces, applied to the buffer.
fn after_enter(marked: &str) -> String {
    let buffer = Buffer::over(NULL, marked);
    let path = repo_root().join(NULL);
    let text = buffer.world.text(&path).expect("text");
    let edits = features::on_type_formatting(&buffer.world, &buffer.uri, buffer.position, "\n")
        .unwrap_or_default();

    let mut out = text.clone();
    // One edit, and applying it back to front would matter if there were more.
    for edit in edits.iter().rev() {
        let start = piton_lsp::convert::position_to_offset(&out, edit.range.start);
        let end = piton_lsp::convert::position_to_offset(&out, edit.range.end);
        out.replace_range(start..end, &edit.new_text);
    }
    out
}

#[test]
fn enter_after_a_key_that_opens_a_block_indents() {
    // `frameworks:` with nothing after it opens a block, and the block is the
    // indented lines beneath it.
    let text = after_enter("export type N:\n    frameworks:\n<|>");
    assert!(
        text.ends_with("    frameworks:\n        "),
        "expected two levels of indent: {text:?}"
    );
}

#[test]
fn enter_after_a_declaration_indents_into_its_body() {
    let text = after_enter("export type N:\n<|>");
    assert!(
        text.ends_with("export type N:\n    "),
        "a declaration opens a body: {text:?}"
    );
}

#[test]
fn enter_after_a_value_keeps_the_indentation() {
    // `title: a book` is a whole property; the line after it is a sibling.
    let text = after_enter("export type N:\n    title: a book\n<|>");
    assert!(
        text.ends_with("    title: a book\n    "),
        "expected the same indent, not a deeper one: {text:?}"
    );
}

#[test]
fn enter_after_a_comment_that_ends_in_a_colon_does_not_indent() {
    // A comment is not structure, whatever it happens to end with.
    let text = after_enter("export type N:\n    // a note about this:\n<|>");
    assert!(
        text.ends_with("// a note about this:\n    "),
        "a comment opens nothing: {text:?}"
    );
}

#[test]
fn enter_after_a_value_that_ends_in_a_colon_does_not_indent() {
    // `prompt: Careful: ` ends a sentence, not a property: the colon is part
    // of the value, and the line after it is a sibling.
    let text = after_enter("export type N:\n    prompt: Careful:\n<|>");
    assert!(
        text.ends_with("    prompt: Careful:\n    "),
        "a colon inside a value opens nothing: {text:?}"
    );
}

#[test]
fn enter_after_a_constraint_that_leaves_the_value_unwritten_does_indent() {
    // `config:: dictionary:` is a key with no value: the colon that closes
    // the constraint is the one that opens the block beneath it.
    let text = after_enter("export type N:\n    config:: dictionary:\n<|>");
    assert!(
        text.ends_with("    config:: dictionary:\n        "),
        "a key with no value opens a block: {text:?}"
    );
}

#[test]
fn enter_replaces_indentation_the_cursor_has_not_reached() {
    // The line the newline landed on may already be indented, with the cursor
    // still at the margin. Replacing only what sits before the cursor would
    // stack the two together and put the line at twelve spaces instead of
    // eight -- a double indent.
    let text = after_enter("export type N:\n    frameworks:\n<|>    title: x");
    assert!(
        text.ends_with("    frameworks:\n        title: x"),
        "expected one level deeper, not stacked: {text:?}"
    );
}

#[test]
fn enter_when_the_client_already_indented_changes_nothing() {
    // Same indent as the one wanted: there is nothing to correct.
    let buffer = Buffer::over(NULL, "export type N:\n    frameworks:\n        <|>");
    let edits = features::on_type_formatting(&buffer.world, &buffer.uri, buffer.position, "\n");
    assert!(edits.as_deref().unwrap_or_default().is_empty(), "{edits:?}");
}

#[test]
fn enter_inside_an_escape_block_changes_nothing() {
    // The contents of an escape block are verbatim, so reindenting them would
    // change what the block says.
    let buffer = Buffer::over(
        NULL,
        "export type N:\n    a:\n        \\\\\\\n        key:\n<|>",
    );
    let edits = features::on_type_formatting(&buffer.world, &buffer.uri, buffer.position, "\n");
    assert!(edits.as_deref().unwrap_or_default().is_empty(), "{edits:?}");
}

#[test]
fn enter_on_a_blank_line_dedents_one_level() {
    // The spec: "On a blank line inside a dictionary or anchor, enter should
    // insert a new line and dedent it by 1 level." The line being left behind
    // (`    `) is blank and indented, so the new line backs out one level to
    // the margin rather than holding the body's depth.
    let text = after_enter("export type N:\n    key: value\n    \n<|>");
    assert!(
        text.ends_with("    key: value\n    \n"),
        "expected the new line to dedent to the margin: {text:?}"
    );
}

#[test]
fn enter_on_a_nested_blank_line_dedents_by_exactly_one_level() {
    // Deeper nesting: from level 2 (8 spaces) back to level 1 (4 spaces), not
    // all the way to the margin.
    let text = after_enter("export type N:\n    outer:\n        inner: 1\n        \n<|>");
    assert!(
        text.ends_with("        inner: 1\n        \n    "),
        "expected one level out, not two: {text:?}"
    );
}

#[test]
fn enter_on_a_blank_line_the_editor_emptied_dedents_by_one_level() {
    // Zed strips the whitespace from a blank line as Enter leaves it, so the
    // blank line arrives empty. It still backs out one level, not to the
    // margin.
    let text = after_enter("export type N:\n    outer:\n        inner: 1\n\n<|>");
    assert!(
        text.ends_with("        inner: 1\n\n    "),
        "expected one level out, not all the way: {text:?}"
    );
    // A second Enter on another emptied blank line backs out one more.
    let text = after_enter("export type N:\n    outer:\n        inner: 1\n\n\n<|>");
    assert!(text.ends_with("        inner: 1\n\n\n"), "{text:?}");
}

#[test]
fn enter_on_a_blank_line_at_the_margin_stays_there() {
    // A blank line already at the margin has nothing to dedent out of, so it is
    // a no-op rather than wrapping to a negative depth. Here the line above the
    // cursor is itself blank and at column zero.
    let buffer = Buffer::over(NULL, "anchor A:\n\n<|>");
    let edits = features::on_type_formatting(&buffer.world, &buffer.uri, buffer.position, "\n");
    assert!(edits.as_deref().unwrap_or_default().is_empty(), "{edits:?}");
}

#[test]
fn only_the_newline_triggers_a_reindent() {
    let buffer = Buffer::over(NULL, "export type N:\n<|>");
    assert!(
        features::on_type_formatting(&buffer.world, &buffer.uri, buffer.position, ":").is_none(),
        "nothing but a newline changes which line the cursor is on"
    );
}

#[test]
fn an_unconstrained_value_proposes_nothing() {
    // The specification is explicit: `property: ` should propose nothing,
    // because the likely intent is to write prose. Only a constraint saying
    // what belongs there makes a proposal something other than a guess.
    // `whatIsAType` carries no constraint at all.
    let free = Buffer::over(
        NULL,
        "use ./lib/Type\n\nexport type Null:\n    whatIsAType: <|>\n",
    );
    assert!(
        free.labels().is_empty(),
        "nothing says what goes here: {:?}",
        free.labels()
    );

    // `description:: simple:: complex` does carry one, but it describes the
    // shape of a value rather than naming which values there are -- so there
    // is still nothing to propose, and proposing every anchor in the project
    // would be the same guess.
    let shaped = Buffer::over(
        NULL,
        "use ./lib/Type\n\nexport type Null:\n    description: <|>\n",
    );
    assert!(
        shaped.labels().is_empty(),
        "`simple` and `complex` name a shape, not a set of values: {:?}",
        shaped.labels()
    );
}

#[test]
fn implementations_lead_from_an_abstract_anchor_to_what_extends_it() {
    let fixture = Fixture::new();
    let uri = fixture.uri("spec/scope/language/types/lib/Type.pi");
    let position = fixture.position_of("spec/scope/language/types/lib/Type.pi", "Type as type");

    let response = features::implementations(&fixture.world, &uri, position);
    let Some(request::GotoImplementationResponse::Array(locations)) = response else {
        panic!("expected implementations of Type");
    };
    // Every `type X` in the specification is one of them.
    assert!(locations.len() > 5, "only {} found", locations.len());
    for location in &locations {
        assert!(
            location.uri.to_string().ends_with(".pi"),
            "{}",
            location.uri
        );
    }
}

#[test]
fn a_leaf_anchor_leads_only_to_what_composes_it() {
    let fixture = Fixture::new();
    let uri = fixture.uri("spec/scope/belay/Compilation.pi");
    // On the anchor's own name, not on the base it extends, which does have
    // implementations.
    let position = fixture.position_of("spec/scope/belay/Compilation.pi", "MarkdownSerialization extends");
    // Nothing extends a concrete leaf. What it does have is the anchors that
    // compose it: `Compilation` writes `serialization: {MarkdownSerialization}`.
    let Some(request::GotoImplementationResponse::Array(locations)) =
        features::implementations(&fixture.world, &uri, position)
    else {
        panic!("the composer of MarkdownSerialization is related to it");
    };
    assert!(
        !locations.is_empty()
            && locations
                .iter()
                .all(|location| location.uri.path().ends_with("belay/Compilation.pi")),
        "{locations:?}"
    );
}

#[test]
fn the_type_hierarchy_walks_both_ways() {
    let fixture = Fixture::new();
    let uri = fixture.uri("spec/scope/language/types/lib/Type.pi");
    let position = fixture.position_of("spec/scope/language/types/lib/Type.pi", "Type as type");

    let prepared =
        features::prepare_type_hierarchy(&fixture.world, &uri, position).expect("a hierarchy item");
    assert_eq!(prepared.len(), 1);
    let item = &prepared[0];
    assert_eq!(item.name, "Type");
    assert!(
        item.detail
            .as_deref()
            .is_some_and(|detail| detail.contains("abstract") && detail.contains("as type")),
        "{:?}",
        item.detail
    );

    // An abstract at the top of its own chain has no supertypes.
    let supertypes = features::type_hierarchy_supertypes(&fixture.world, item).expect("supertypes");
    assert!(supertypes.is_empty(), "{supertypes:?}");

    let subtypes = features::type_hierarchy_subtypes(&fixture.world, item).expect("subtypes");
    let names: Vec<&str> = subtypes.iter().map(|item| item.name.as_str()).collect();
    assert!(names.contains(&"Strings"), "{names:?}");

    // Walking down and then back up returns to where it started.
    let back = features::type_hierarchy_supertypes(&fixture.world, &subtypes[0])
        .expect("supertypes of a subtype");
    assert!(
        back.iter().any(|parent| parent.name == "Type"),
        "{:?}",
        back.iter().map(|item| &item.name).collect::<Vec<_>>()
    );
}

#[test]
fn a_hierarchy_item_survives_a_lost_identity() {
    let fixture = Fixture::new();
    let uri = fixture.uri("spec/scope/language/types/Strings.pi");
    let position = fixture.position_of("spec/scope/language/types/Strings.pi", "Strings:");
    let mut item = features::prepare_type_hierarchy(&fixture.world, &uri, position)
        .expect("a hierarchy item")
        .remove(0);
    assert_eq!(item.name, "Strings");

    // An item prepared before a recompilation carries an id that may no longer
    // mean anything; the name still does.
    item.data = None;
    let supertypes =
        features::type_hierarchy_supertypes(&fixture.world, &item).expect("supertypes");
    assert!(
        supertypes.iter().any(|parent| parent.name == "Type"),
        "{:?}",
        supertypes.iter().map(|item| &item.name).collect::<Vec<_>>()
    );
}

#[test]
fn enter_after_a_list_item_ending_in_a_colon_does_not_indent() {
    // A list item is never a key: `- Settings:` is just the string
    // `Settings:`, so it opens nothing.
    let text = after_enter("export type N:\n    items:\n        - Settings:\n<|>");
    assert!(
        text.ends_with("        - Settings:\n        "),
        "a list item opens no block: {text:?}"
    );
}

#[test]
fn enter_after_a_merge_line_does_not_indent() {
    let text = after_enter("export type N:\n    items:\n        + {x}:\n<|>");
    assert!(
        text.ends_with("        + {x}:\n        "),
        "a merge line opens no block: {text:?}"
    );
}

#[test]
fn super_offers_what_every_base_has() {
    let buffer = Buffer::over(
        NULL,
        "anchor Left:\n    fromLeft: a\n\nanchor Right:\n    fromRight: b\n\nexport anchor Child extends Left, Right:\n    x: ${super.<|>}\n",
    );
    let labels = buffer.labels();
    assert!(labels.contains(&"fromLeft".to_string()), "{labels:?}");
    assert!(labels.contains(&"fromRight".to_string()), "{labels:?}");
}

#[test]
fn a_nested_dictionary_is_not_offered_the_anchors_properties() {
    let buffer = Buffer::over(
        NULL,
        "use ./lib/Type\n\nexport type Null:\n    config:\n        <|>\n",
    );
    assert!(
        buffer.labels().is_empty(),
        "the anchor's own properties do not go inside `config`: {:?}",
        buffer.labels()
    );
    // Directly in the body they still are.
    let body = Buffer::over(NULL, "use ./lib/Type\n\nexport type Null:\n    <|>\n");
    assert!(body.labels().contains(&"description".to_string()));
}

#[test]
fn semantic_tokens_mark_types_and_expression_operators() {
    let buffer = Buffer::over(
        NULL,
        "<|>use ./lib/Type\n\nexport type Null:\n    n:: number: {1 + 2}\n    s: ${\"a\" ++ \"b\"}\n",
    );
    let Some(SemanticTokensResult::Tokens(tokens)) =
        features::semantic_tokens(&buffer.world, &buffer.uri)
    else {
        panic!("tokens");
    };
    // Decode to (line, column, length, type).
    let mut decoded = Vec::new();
    let (mut line, mut column) = (0u32, 0u32);
    for token in &tokens.data {
        if token.delta_line > 0 {
            line += token.delta_line;
            column = token.delta_start;
        } else {
            column += token.delta_start;
        }
        decoded.push((line, column, token.length, token.token_type));
    }
    let lines: Vec<&str> = buffer.text.lines().collect();
    let text_of = |(line, column, length, _): &(u32, u32, u32, u32)| -> String {
        lines[*line as usize]
            .chars()
            .skip(*column as usize)
            .take(*length as usize)
            .collect()
    };
    let kind = |wanted: &str| -> Vec<u32> {
        decoded
            .iter()
            .filter(|token| text_of(token) == wanted)
            .map(|token| token.3)
            .collect()
    };
    let type_index = TOKEN_TYPES
        .iter()
        .position(|kind| *kind == SemanticTokenType::TYPE)
        .unwrap() as u32;
    let operator = TOKEN_TYPES
        .iter()
        .position(|kind| *kind == SemanticTokenType::OPERATOR)
        .unwrap() as u32;
    let string = TOKEN_TYPES
        .iter()
        .position(|kind| *kind == SemanticTokenType::STRING)
        .unwrap() as u32;
    assert!(kind("number").contains(&type_index), "{decoded:?}");
    assert!(kind("+").contains(&operator), "{decoded:?}");
    assert!(kind("++").contains(&operator), "{decoded:?}");
    assert!(kind("\"a\"").contains(&string), "{decoded:?}");
    assert!(kind("${").contains(&operator), "{decoded:?}");
}

#[test]
fn workspace_symbols_carry_their_description() {
    let fixture = Fixture::new();
    let symbols = features::workspace_symbols(&fixture.world, "Strings").expect("symbols");
    let strings = symbols
        .iter()
        .find(|symbol| symbol.name == "Strings")
        .expect("Strings");
    assert!(
        strings
            .container_name
            .as_deref()
            .is_some_and(|container| container.contains("Strings are not quoted")),
        "{:?}",
        strings.container_name
    );
}

#[test]
fn the_type_hierarchy_includes_composition() {
    let fixture = Fixture::new();
    let uri = fixture.uri("spec/scope/belay/Compilation.pi");
    let position = fixture.position_of("spec/scope/belay/Compilation.pi", "MarkdownSerialization extends");
    let item = features::prepare_type_hierarchy(&fixture.world, &uri, position)
        .expect("item")
        .remove(0);
    let subtypes = features::type_hierarchy_subtypes(&fixture.world, &item).expect("subtypes");
    let composer = subtypes
        .iter()
        .find(|item| item.name == "Compilation")
        .unwrap_or_else(|| panic!("{:?}", subtypes.iter().map(|i| &i.name).collect::<Vec<_>>()));
    assert!(
        composer
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("composes it")),
        "{:?}",
        composer.detail
    );
}
