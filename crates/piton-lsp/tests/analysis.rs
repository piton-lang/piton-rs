//! Import organization, composition, conflicts, unused symbols, and the
//! source-to-output map. Each fixture is a project of its own so a finding
//! here is a finding in the analysis, not a quirk of the specification corpus.

use std::path::PathBuf;

use piton_lsp::convert::{offset_to_position, path_to_url};
use piton_lsp::features;
use piton_lsp::world::World;
use tower_lsp::lsp_types::*;

struct Fixture {
    dir: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let dir =
            std::env::temp_dir().join(format!("piton-lsp-analysis-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("spec")).expect("temp dir");
        Fixture { dir }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, contents).expect("write");
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.join(relative)
    }

    fn world(&self) -> World {
        let mut world = World::new(&self.dir);
        world.recompile(None);
        world
    }

    fn uri(&self, relative: &str) -> Url {
        path_to_url(&self.path(relative)).expect("url")
    }

    fn position(&self, relative: &str, needle: &str) -> Position {
        let text = std::fs::read_to_string(self.path(relative)).expect("readable");
        let offset = text
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` not in {relative}"));
        offset_to_position(&text, offset + needle.len() / 2)
    }
}

fn codes(world: &World, relative: &str, fixture: &Fixture) -> Vec<String> {
    world
        .diagnostics_by_file()
        .get(&fixture.path(relative))
        .map(|items| items.iter().map(|item| item.code.clone()).collect())
        .unwrap_or_default()
}

fn apply(text: &str, edits: &[TextEdit]) -> String {
    let mut owned = text.to_string();
    let mut ordered = edits.to_vec();
    ordered.sort_by_key(|edit| std::cmp::Reverse(edit.range.start));
    for edit in ordered {
        let start = piton_lsp::convert::position_to_offset(&owned, edit.range.start);
        let end = piton_lsp::convert::position_to_offset(&owned, edit.range.end);
        owned.replace_range(start..end, &edit.new_text);
    }
    owned
}

#[test]
fn unused_duplicate_and_broad_imports_are_reported_and_cleaned() {
    let fixture = Fixture::new("imports");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
    );
    fixture.write(
        "spec/Other.pi",
        "export anchor Alpha:\n    description: first\n\nexport anchor Beta:\n    description: second\n\nexport anchor Gamma:\n    description: third\n",
    );
    fixture.write(
        "spec/index.pi",
        "from ./Other import Beta, Alpha, Beta, Gamma\nfrom ./Missing import Nowhere\n\nexport anchor Root:\n    description: uses {Alpha} and {Beta}\n",
    );

    let world = fixture.world();
    let reported = codes(&world, "spec/index.pi", &fixture);
    assert!(
        reported.iter().any(|code| code == "unused-import"),
        "Gamma is imported and never referenced: {reported:?}"
    );
    assert!(
        reported.iter().any(|code| code == "duplicate-import"),
        "Beta is imported twice: {reported:?}"
    );
    assert!(
        reported.iter().any(|code| code == "broad-import"),
        "the import lists a name the file does not use: {reported:?}"
    );

    let uri = fixture.uri("spec/index.pi");
    let actions = features::code_actions(&world, &uri, Range::default()).expect("actions");
    let organize = actions.iter().find_map(|action| match action {
        CodeActionOrCommand::CodeAction(action) if action.title == "Organize imports" => {
            Some(action)
        }
        _ => None,
    });
    let organize = organize.expect("an organize-imports action");
    assert_eq!(organize.kind, Some(CodeActionKind::SOURCE_ORGANIZE_IMPORTS));
    let edits = organize
        .edit
        .as_ref()
        .and_then(|edit| edit.changes.as_ref())
        .and_then(|changes| changes.get(&uri))
        .expect("edits");
    let text = std::fs::read_to_string(fixture.path("spec/index.pi")).expect("readable");
    let cleaned = apply(&text, edits);
    assert!(
        cleaned.contains("from ./Other import Alpha, Beta"),
        "kept names are sorted and the unused, duplicate, and invalid ones are gone:\n{cleaned}"
    );
    assert!(!cleaned.contains("Gamma"), "{cleaned}");
    assert!(!cleaned.contains("Missing"), "{cleaned}");
    assert!(!cleaned.contains("Nowhere"), "{cleaned}");
}

#[test]
fn an_unused_use_is_an_unused_import() {
    let fixture = Fixture::new("use");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
    );
    fixture.write(
        "spec/Keywords.pi",
        "export anchor Widget as widget:\n    description: a keyword\n",
    );
    fixture.write(
        "spec/index.pi",
        "use ./Keywords\n\nexport anchor Root:\n    description: never uses the keyword\n",
    );

    let world = fixture.world();
    let reported = codes(&world, "spec/index.pi", &fixture);
    assert!(
        reported.iter().any(|code| code == "unused-import"),
        "{reported:?}"
    );
}

#[test]
fn composition_hover_shows_how_inputs_combine() {
    let fixture = Fixture::new("compose");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
    );
    fixture.write(
        "spec/index.pi",
        "extra:\n    - two\n    - three\n\nexport anchor Combined:\n    items:\n        - one\n        + {extra}\n",
    );

    let world = fixture.world();
    let uri = fixture.uri("spec/index.pi");
    let hover = features::hover(&world, &uri, fixture.position("spec/index.pi", "+ {extra}"))
        .expect("hover on the merge");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markup");
    };
    assert!(
        markup.value.contains("Composition"),
        "the hover should explain the combination:\n{}",
        markup.value
    );
    assert!(
        markup.value.contains("extra") || markup.value.contains("merge"),
        "{}",
        markup.value
    );

    let hints = features::inlay_hints(
        &world,
        &uri,
        Range {
            start: Position::default(),
            end: Position {
                line: 20,
                character: 0,
            },
        },
    )
    .expect("hints");
    let labels: Vec<String> = hints
        .iter()
        .map(|hint| match &hint.label {
            InlayHintLabel::String(text) => text.clone(),
            _ => String::new(),
        })
        .collect();
    assert!(
        labels.iter().any(|label| label.contains("merge")),
        "a merge should be marked inline: {labels:?}"
    );
}

#[test]
fn incompatible_abstract_constraints_are_a_conflict() {
    let fixture = Fixture::new("conflict");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
    );
    fixture.write(
        "spec/index.pi",
        "abstract anchor Left:\n    value:: string\n\nabstract anchor Right:\n    value:: number\n\nexport anchor Child extends Left, Right:\n    value: 1\n",
    );

    let world = fixture.world();
    let reported = world
        .diagnostics_by_file()
        .get(&fixture.path("spec/index.pi"))
        .cloned()
        .unwrap_or_default();
    let conflict = reported
        .iter()
        .find(|item| item.code == "inheritance-conflict")
        .expect("a constraint disagreement is a conflict");
    assert_eq!(conflict.severity, piton_core::Severity::Error);
    assert!(conflict.message.contains("value"), "{}", conflict.message);
}

#[test]
fn a_repeated_inherited_value_is_redundant() {
    let fixture = Fixture::new("redundant");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
    );
    fixture.write(
        "spec/index.pi",
        "export anchor Base:\n    description: same text\n\nexport anchor Child extends Base:\n    description: same text\n    extra: kept\n",
    );

    let world = fixture.world();
    let reported = codes(&world, "spec/index.pi", &fixture);
    assert!(
        reported.iter().any(|code| code == "redundant-definition"),
        "{reported:?}"
    );
    assert_eq!(
        reported
            .iter()
            .filter(|code| *code == "redundant-definition")
            .count(),
        1,
        "only the repeated property, not `extra`: {reported:?}"
    );
}

#[test]
fn an_unreferenced_anchor_is_unused() {
    let fixture = Fixture::new("unused");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
    );
    fixture.write(
        "spec/index.pi",
        "export anchor Used:\n    description: the entry exports this\n",
    );
    fixture.write(
        "spec/Orphan.pi",
        "export anchor Nobody:\n    description: nothing reaches this\n",
    );

    let world = fixture.world();
    let reported = codes(&world, "spec/Orphan.pi", &fixture);
    assert!(
        reported.iter().any(|code| code == "unused-anchor"),
        "{reported:?}"
    );
    let entry = codes(&world, "spec/index.pi", &fixture);
    assert!(
        !entry.iter().any(|code| code == "unused-anchor"),
        "the entry export is the compiled result: {entry:?}"
    );
}

#[test]
fn source_maps_to_the_compiled_section_and_back() {
    let fixture = Fixture::new("mapping");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
    );
    fixture.write(
        "spec/index.pi",
        "export anchor Hello:\n    description: A mapped property\n",
    );

    let world = fixture.world();
    let source = fixture.path("spec/index.pi");
    let text = std::fs::read_to_string(&source).expect("readable");
    let offset = text.find("description").expect("property") + 2;
    let mappings = world.analysis.mappings_at(&source, offset);
    let mapping = mappings
        .iter()
        .find(|mapping| mapping.adapter == "markdown")
        .expect("the property maps to compiled markdown");
    assert!(
        mapping.output_path.ends_with("index.md"),
        "{}",
        mapping.output_path.display()
    );
    assert!(
        mappings.iter().any(|mapping| mapping.adapter == "json"),
        "json output is mapped too"
    );

    let back = world
        .analysis
        .at_output(&mapping.output_path, mapping.output_start)
        .expect("the compiled slice maps back");
    assert!(
        back.source_span.contains(offset) || back.source_file.ends_with("index.pi"),
        "output should point at the source construct"
    );

    let uri = fixture.uri("spec/index.pi");
    let lenses = features::code_lenses(&world, &uri).expect("lenses");
    assert!(
        lenses.iter().any(|lens| {
            lens.command
                .as_ref()
                .is_some_and(|command| command.title.contains("markdown"))
        }),
        "a code lens should name the compiled output: {lenses:?}"
    );

    let query = features::source_to_output(
        &world,
        &serde_json::json!({
            "uri": uri,
            "line": 1,
            "character": 4,
        }),
    );
    assert!(
        query.as_array().is_some_and(|items| !items.is_empty()),
        "the custom query should return the mapping: {query}"
    );
}

#[test]
fn belay_plan_checks_reach_the_editor() {
    let fixture = Fixture::new("belay");
    std::fs::create_dir_all(fixture.path("src")).expect("code root");
    let long = "x".repeat(1100);
    fixture.write(
        "piton.config.pi",
        "use @piton/config\nuse @piton/belay\n\nfrom @piton/belay import ClaudeAdapter\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n    frameworks:\n        - {BelayConfiguration}\n\nbelay-config BelayConfiguration:\n    codeRoot: ./src\n    adapters:\n        - {ClaudeAdapter}\n",
    );
    fixture.write(
        "spec/index.pi",
        &format!(
            "use @piton/belay\n\nexport skill Verbose:\n    description: {long}\n    useWhen: when the description is too long to discover\n    prompt: hello\n"
        ),
    );

    let world = fixture.world();
    let reported = codes(&world, "spec/index.pi", &fixture);
    assert!(
        reported.iter().any(|code| code == "description-too-long"),
        "Belay plan validation should be visible in the editor: {reported:?}"
    );
}

#[test]
fn a_list_merged_with_a_dictionary_is_a_composition_conflict() {
    let fixture = Fixture::new("compose-conflict");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
    );
    fixture.write(
        "spec/index.pi",
        "items:\n    - one\n\nbag:\n    key: value\n\nexport anchor Broken:\n    description: {items + bag}\n",
    );

    let world = fixture.world();
    let reported = codes(&world, "spec/index.pi", &fixture);
    assert!(
        reported.iter().any(|code| code == "composition-conflict"),
        "{reported:?}"
    );
}
