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

const CONFIG: &str =
    "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n";

#[test]
fn only_abstracts_that_cannot_both_hold_conflict() {
    let fixture = Fixture::new("conflict");
    fixture.write("piton.config.pi", CONFIG);
    fixture.write(
        "spec/index.pi",
        "abstract anchor Left:\n    value:: string\n\nabstract anchor Right:\n    value:: number\n\nexport anchor Child extends Left, Right:\n    value: 1\n",
    );

    let world = fixture.world();
    let reported = codes(&world, "spec/index.pi", &fixture);
    // The compiler reports it; the editor does not add a second squiggle.
    assert!(
        reported.iter().any(|code| code == "conflicting-abstracts"),
        "{reported:?}"
    );
    assert!(
        !reported.iter().any(|code| code == "inheritance-conflict"),
        "{reported:?}"
    );

    // The fix is to implement one of them, not both.
    let uri = fixture.uri("spec/index.pi");
    let titles = action_titles(&world, &uri, fixture.position("spec/index.pi", "Child"));
    assert!(
        titles.iter().any(|title| title == "Stop extending `Left`"),
        "{titles:?}"
    );
}

#[test]
fn overlapping_abstracts_and_concrete_bases_do_not_conflict() {
    let fixture = Fixture::new("no-conflict");
    fixture.write("piton.config.pi", CONFIG);
    fixture.write(
        "spec/index.pi",
        "abstract anchor Left:\n    value:: string:: number\n\nabstract anchor Right:\n    value:: number\n\nexport anchor Both extends Left, Right:\n    value: 1\n\nexport anchor A:\n    note:: string: a\n\nexport anchor B:\n    note:: number: 2\n\nexport anchor Concrete extends A, B:\n    pass\n",
    );

    let world = fixture.world();
    let reported = codes(&world, "spec/index.pi", &fixture);
    assert!(
        !reported
            .iter()
            .any(|code| code == "conflicting-abstracts" || code == "inheritance-conflict"),
        "overlapping abstracts and concrete bases resolve right-most-wins: {reported:?}"
    );
}

#[test]
fn a_simple_inheritance_conflict_can_be_resolved_from_the_header() {
    let fixture = Fixture::new("resolve-conflict");
    fixture.write("piton.config.pi", CONFIG);
    fixture.write(
        "spec/index.pi",
        "export anchor A:\n    tone: calm\n\nexport anchor B:\n    tone: loud\n\nexport anchor Child extends A, B:\n    other: x\n",
    );
    let world = fixture.world();
    let uri = fixture.uri("spec/index.pi");
    let actions = features::code_actions(
        &world,
        &uri,
        Range {
            start: fixture.position("spec/index.pi", "Child extends"),
            end: fixture.position("spec/index.pi", "Child extends"),
        },
    )
    .expect("actions");
    let text = std::fs::read_to_string(fixture.path("spec/index.pi")).expect("readable");
    let applied = |title: &str| -> String {
        let action = actions
            .iter()
            .find_map(|action| match action {
                CodeActionOrCommand::CodeAction(action) if action.title == title => Some(action),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no `{title}` in {:?}", titles_of(&actions)));
        let edits = action
            .edit
            .as_ref()
            .and_then(|edit| edit.changes.as_ref())
            .and_then(|changes| changes.get(&uri))
            .expect("edits");
        apply(&text, edits)
    };
    let reordered = applied("Move `A` last in `extends` so its `tone` wins");
    assert!(reordered.contains("anchor Child extends B, A:"), "{reordered}");
    let taken = applied("Take `tone` from `A` (it currently comes from `B`)");
    assert!(
        taken.contains("anchor Child extends A, B:\n    tone: {A.tone}\n    other: x"),
        "{taken}"
    );
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
    fixture.write("piton.config.pi", CONFIG);
    fixture.write(
        "spec/index.pi",
        "export anchor Hello:\n    description: A mapped property\n",
    );

    let world = fixture.world();
    let source = fixture.path("spec/index.pi");
    let text = std::fs::read_to_string(&source).expect("readable");
    let offset = text.find("description").expect("property") + 2;
    let mappings = world.analysis.mappings_at(&source, offset);
    // By default the build writes JSON to ./dist, mirroring the source root.
    let mapping = mappings
        .iter()
        .find(|mapping| mapping.adapter == "json")
        .expect("the property maps to the compiled JSON");
    assert_eq!(mapping.output_path, fixture.path("dist/index.json"));
    assert!(
        !mappings.iter().any(|mapping| mapping.adapter == "markdown"),
        "only the configured renderer is written"
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
    let lens = lenses
        .iter()
        .filter_map(|lens| lens.command.as_ref())
        .find(|command| command.title.contains("json"))
        .unwrap_or_else(|| panic!("a code lens should name the compiled output: {lenses:?}"));
    // The lens runs a command the server itself executes.
    assert_eq!(lens.command, features::SOURCE_TO_OUTPUT);
    let outcome = features::execute_command(
        &world,
        &lens.command,
        lens.arguments.as_deref().unwrap_or_default(),
    );
    assert!(
        matches!(outcome, Some(features::CommandOutcome::Message(ref message)) if message.contains("piton compile")),
        "nothing is compiled yet, so the command says how to get there: {outcome:?}"
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

    // The configuration is read, not compiled: it maps to nothing.
    let config = fixture.path("piton.config.pi");
    assert!(world
        .analysis
        .mappings
        .iter()
        .all(|mapping| mapping.source_file != config));
}

#[test]
fn the_configured_output_directory_and_renderer_decide_the_mapping() {
    let fixture = Fixture::new("mapping-md");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n    output: ./out\n    renderer: markdown\n",
    );
    fixture.write(
        "spec/nested/Page.pi",
        "export anchor Page:\n    description: A page\n",
    );
    fixture.write("spec/index.pi", "from ./nested/Page export Page\n");

    let world = fixture.world();
    let source = fixture.path("spec/nested/Page.pi");
    let outputs: Vec<(&str, &std::path::Path)> = world
        .analysis
        .mappings
        .iter()
        .filter(|mapping| mapping.source_file == source)
        .map(|mapping| (mapping.adapter.as_str(), mapping.output_path.as_path()))
        .collect();
    // Page.pi compiles to out/nested/Page.md, and the index that re-exports
    // it carries it into out/index.md too.
    assert!(
        outputs.contains(&("markdown", fixture.path("out/nested/Page.md").as_path())),
        "{outputs:?}"
    );
    assert!(
        outputs.contains(&("markdown", fixture.path("out/index.md").as_path())),
        "{outputs:?}"
    );
    assert!(outputs.iter().all(|(adapter, _)| *adapter == "markdown"));
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
fn a_list_merged_with_a_dictionary_is_the_compilers_error_to_report() {
    let fixture = Fixture::new("compose-conflict");
    fixture.write("piton.config.pi", CONFIG);
    fixture.write(
        "spec/index.pi",
        "items:\n    - one\n\nbag:\n    key: value\n\nexport anchor Broken:\n    description: {items + bag}\n",
    );

    let world = fixture.world();
    let reported = codes(&world, "spec/index.pi", &fixture);
    assert!(
        reported.iter().any(|code| code == "invalid-operand"),
        "{reported:?}"
    );
    assert!(
        !reported.iter().any(|code| code == "composition-conflict"),
        "the editor does not repeat the compiler: {reported:?}"
    );
    let hover = hover_text(&world, &fixture, "spec/index.pi", "items + bag");
    assert!(hover.contains("cannot combine list and dictionary"), "{hover}");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn titles_of(actions: &[CodeActionOrCommand]) -> Vec<String> {
    actions
        .iter()
        .map(|action| match action {
            CodeActionOrCommand::CodeAction(action) => action.title.clone(),
            CodeActionOrCommand::Command(command) => command.title.clone(),
        })
        .collect()
}

fn action_titles(world: &World, uri: &Url, position: Position) -> Vec<String> {
    let actions = features::code_actions(
        world,
        uri,
        Range {
            start: position,
            end: position,
        },
    )
    .unwrap_or_default();
    titles_of(&actions)
}

fn hover_text(world: &World, fixture: &Fixture, relative: &str, needle: &str) -> String {
    let hover = features::hover(world, &fixture.uri(relative), fixture.position(relative, needle))
        .unwrap_or_else(|| panic!("no hover at `{needle}`"));
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markup");
    };
    markup.value
}

/// The position of the `n`th occurrence of `needle` (0-based), at its start.
fn nth_position(fixture: &Fixture, relative: &str, needle: &str, n: usize) -> Position {
    let text = std::fs::read_to_string(fixture.path(relative)).expect("readable");
    let offset = text
        .match_indices(needle)
        .nth(n)
        .unwrap_or_else(|| panic!("no occurrence {n} of `{needle}`"))
        .0;
    offset_to_position(&text, offset + 1)
}

// ---------------------------------------------------------------------------
// Composition follows the value kinds
// ---------------------------------------------------------------------------

#[test]
fn string_blocks_join_rather_than_merge() {
    let fixture = Fixture::new("string-join");
    fixture.write("piton.config.pi", CONFIG);
    fixture.write(
        "spec/index.pi",
        "export anchor Base:\n    d: Base text.\n\nexport anchor Plus extends Base:\n    d:\n        Child text.\n        + {super.d}\n\nexport anchor DoublePlus extends Base:\n    d:\n        Child text.\n        ++ {super.d}\n",
    );
    let world = fixture.world();

    let plus = hover_text(&world, &fixture, "spec/index.pi", "+ {super.d}");
    assert!(plus.contains("joins the strings with nothing in between"), "{plus}");
    assert!(plus.contains("Child text.Base text."), "{plus}");
    assert!(!plus.contains("duplicates"), "strings are not merged: {plus}");

    let double = hover_text(&world, &fixture, "spec/index.pi", "++ {super.d}");
    assert!(double.contains("line break"), "{double}");
}

#[test]
fn super_in_a_block_reads_every_base() {
    let fixture = Fixture::new("super-bases");
    fixture.write("piton.config.pi", CONFIG);
    fixture.write(
        "spec/index.pi",
        "export anchor Left:\n    items:\n        - a\n\nexport anchor Right:\n    other: x\n\nexport anchor Child extends Left, Right:\n    items:\n        + {super.items}\n        - b\n",
    );
    let world = fixture.world();
    let hover = hover_text(&world, &fixture, "spec/index.pi", "+ {super.items}");
    // `Right` has no `items`, so `super.items` comes from `Left`.
    assert!(hover.contains("[\"a\"]"), "{hover}");
    assert!(hover.contains("- a") && hover.contains("- b"), "{hover}");
}

#[test]
fn dictionaries_merge_shallowly_with_plus_and_deeply_with_double_plus() {
    let fixture = Fixture::new("dict-merge");
    fixture.write("piton.config.pi", CONFIG);
    fixture.write(
        "spec/index.pi",
        "left:\n    outer:\n        a: 1\n\nright:\n    outer:\n        b: 2\n\nexport anchor D:\n    deep: {left ++ right}\n    shallow: {left + right}\n",
    );
    let world = fixture.world();
    let deep = hover_text(&world, &fixture, "spec/index.pi", "left ++ right");
    assert!(deep.contains("deeply"), "{deep}");
    let shallow = hover_text(&world, &fixture, "spec/index.pi", "left + right");
    assert!(shallow.contains("shallowly"), "{shallow}");
}

#[test]
fn a_merge_line_is_redundant_only_when_it_changes_nothing() {
    let fixture = Fixture::new("merge-redundant");
    fixture.write("piton.config.pi", CONFIG);
    fixture.write(
        "spec/index.pi",
        "export anchor Base:\n    items:\n        - A\n        - B\n        - C\n\nexport anchor Reordered extends Base:\n    items:\n        - A\n        - B\n        - C\n        - D\n        + {super.items}\n\nexport anchor Same:\n    items:\n        - A\n        - B\n        + {[\"B\"]}\n",
    );
    let world = fixture.world();
    let reported = world
        .diagnostics_by_file()
        .get(&fixture.path("spec/index.pi"))
        .cloned()
        .unwrap_or_default();
    let redundant: Vec<_> = reported
        .iter()
        .filter(|item| item.code == "redundant-definition")
        .collect();
    // `+` keeps the last of each duplicate, so re-adding A, B, C moves them
    // after D: that line changes the order and is not redundant.
    assert_eq!(redundant.len(), 1, "{reported:#?}");
    let text = std::fs::read_to_string(fixture.path("spec/index.pi")).expect("readable");
    assert_eq!(
        text[redundant[0].span.start..redundant[0].span.end].trim(),
        "+ {[\"B\"]}"
    );
}

// ---------------------------------------------------------------------------
// The project configuration
// ---------------------------------------------------------------------------

#[test]
fn the_configuration_is_a_project_config_not_a_regular_file() {
    let fixture = Fixture::new("config");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n    colour: blue\n",
    );
    fixture.write("spec/index.pi", "export anchor Root:\n    a: 1\n");

    // Open the configuration, the way an editor would.
    let config = fixture.path("piton.config.pi");
    let text = std::fs::read_to_string(&config).expect("readable");
    let mut world = World::new(&fixture.dir);
    world.set_document(config.clone(), format!("{text}    ren"));
    world.recompile(Some(&config));

    let reported = codes(&world, "piton.config.pi", &fixture);
    assert_eq!(
        reported
            .iter()
            .filter(|code| *code == "unknown-config-key")
            .count(),
        1,
        "an unknown setting is reported once: {reported:?}"
    );
    assert!(
        !reported
            .iter()
            .any(|code| code.starts_with("unused")),
        "nothing in the configuration is unused: {reported:?}"
    );

    let uri = fixture.uri("piton.config.pi");
    let hover = features::hover(&world, &uri, fixture.position("piton.config.pi", "root:"))
        .map(|hover| match hover.contents {
            HoverContents::Markup(markup) => markup.value,
            _ => String::new(),
        })
        .unwrap_or_default();
    assert!(!hover.contains("Compiled to"), "{hover}");

    // The settings come from the compiled @piton/config schema.
    let buffer = world.text(&config).expect("text");
    let completion = features::completion(
        &world,
        &uri,
        offset_to_position(&buffer, buffer.len()),
    );
    let labels: Vec<String> = match completion {
        Some(CompletionResponse::Array(items)) => items.into_iter().map(|item| item.label).collect(),
        _ => Vec::new(),
    };
    for key in ["root", "entry", "output", "renderer", "frameworks"] {
        assert!(labels.contains(&key.to_string()), "{key} missing: {labels:?}");
    }
}

// ---------------------------------------------------------------------------
// Ambiguous references
// ---------------------------------------------------------------------------

#[test]
fn an_ambiguous_name_is_reported_and_can_be_qualified() {
    let fixture = Fixture::new("ambiguous");
    fixture.write("piton.config.pi", CONFIG);
    fixture.write("spec/a.pi", "export anchor Button:\n    tone: a\n");
    fixture.write("spec/b.pi", "export anchor Button:\n    tone: b\n");
    fixture.write(
        "spec/index.pi",
        "from ./a import Button\nfrom ./b import Button\n\nexport anchor Page:\n    main: {Button}\n",
    );
    let world = fixture.world();
    let reported = world
        .diagnostics_by_file()
        .get(&fixture.path("spec/index.pi"))
        .cloned()
        .unwrap_or_default();
    let ambiguous = reported
        .iter()
        .find(|item| item.code == "ambiguous-reference")
        .unwrap_or_else(|| panic!("{reported:#?}"));
    assert!(ambiguous.message.contains("./a") && ambiguous.message.contains("./b"));
    assert!(reported.iter().any(|item| item.code == "shadowed-import"));

    // Related information points at each binding, on its own line.
    let text = std::fs::read_to_string(fixture.path("spec/index.pi")).expect("readable");
    let converted = features::to_lsp_diagnostic(ambiguous, &text, &|_| None);
    let lines: Vec<u32> = converted
        .related_information
        .unwrap_or_default()
        .iter()
        .map(|info| info.location.range.start.line)
        .collect();
    assert_eq!(lines, vec![0, 1]);

    let uri = fixture.uri("spec/index.pi");
    let position = fixture.position("spec/index.pi", "{Button}");
    let actions = features::code_actions(
        &world,
        &uri,
        Range {
            start: position,
            end: position,
        },
    )
    .expect("actions");
    let qualify = actions
        .iter()
        .find_map(|action| match action {
            CodeActionOrCommand::CodeAction(action)
                if action.title == "Qualify `Button` as `AButton` (the one from `./a`)" =>
            {
                Some(action)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{:?}", titles_of(&actions)));
    let edits = qualify
        .edit
        .as_ref()
        .and_then(|edit| edit.changes.as_ref())
        .and_then(|changes| changes.get(&uri))
        .expect("edits");
    let qualified = apply(&text, edits);
    assert!(qualified.contains("from ./a import Button AButton\n"), "{qualified}");
    assert!(qualified.contains("main: {AButton}"), "{qualified}");
}

#[test]
fn a_label_in_another_file_lands_on_its_line_there() {
    let other = std::env::temp_dir().join("piton-lsp-label-other.pi");
    let diagnostic = piton_core::Diagnostic::warning(
        "x",
        "message",
        "/here.pi",
        piton_core::Span::new(0, 1),
    )
    .with_label(piton_core::Label::new(
        other.clone(),
        piton_core::Span::new(6, 9),
        "there",
    ));
    let converted = features::to_lsp_diagnostic(&diagnostic, "a\n", &|path| {
        (path == other.as_path()).then(|| "one\ntwo three\n".to_string())
    });
    let info = &converted.related_information.expect("labels")[0];
    assert_eq!(info.location.range.start, Position { line: 1, character: 2 });
    assert_eq!(info.location.range.end, Position { line: 1, character: 5 });
}

// ---------------------------------------------------------------------------
// Overrides and nested keys
// ---------------------------------------------------------------------------

#[test]
fn an_override_leads_to_what_it_replaces_and_back() {
    let fixture = Fixture::new("override");
    fixture.write("piton.config.pi", CONFIG);
    fixture.write(
        "spec/index.pi",
        "export anchor Base:\n    tone: calm\n\nexport anchor Child extends Base:\n    tone: loud\n",
    );
    let world = fixture.world();
    let uri = fixture.uri("spec/index.pi");

    let Some(GotoDefinitionResponse::Scalar(location)) =
        features::definition(&world, &uri, nth_position(&fixture, "spec/index.pi", "tone", 1))
    else {
        panic!("the override leads somewhere");
    };
    assert_eq!(location.range.start.line, 1, "Base's `tone`");

    let Some(request::GotoImplementationResponse::Array(overrides)) =
        features::implementations(&world, &uri, nth_position(&fixture, "spec/index.pi", "tone", 0))
    else {
        panic!("the base property leads to its overrides");
    };
    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides[0].range.start.line, 4, "Child's `tone`");

    let lenses = features::code_lenses(&world, &uri).expect("lenses");
    assert!(
        lenses.iter().any(|lens| lens
            .command
            .as_ref()
            .is_some_and(|command| command.title == "overrides Base.tone"
                && command.command == features::SHOW_LOCATION)),
        "{lenses:?}"
    );
}

#[test]
fn a_nested_key_is_not_an_anchor_property() {
    let fixture = Fixture::new("nested");
    fixture.write("piton.config.pi", CONFIG);
    fixture.write(
        "spec/index.pi",
        "export anchor Card:\n    description: top\n    config:\n        description: nested\n    read: {this.config.description}\n",
    );
    let world = fixture.world();
    let uri = fixture.uri("spec/index.pi");

    let nested = nth_position(&fixture, "spec/index.pi", "description", 1);
    let hover = features::hover(&world, &uri, nested)
        .map(|hover| match hover.contents {
            HoverContents::Markup(markup) => markup.value,
            _ => String::new(),
        })
        .unwrap_or_default();
    assert!(hover.contains("A key of that dictionary"), "{hover}");

    let references = features::references(&world, &uri, nested, true).expect("references");
    let lines: Vec<u32> = references
        .iter()
        .map(|location| location.range.start.line)
        .collect();
    // The nested declaration and the read through `this.config`, but never the
    // anchor's own `description` on line 1.
    assert_eq!(lines, vec![3, 4], "{references:?}");
}

#[test]
fn organizing_imports_drops_the_extension() {
    let fixture = Fixture::new("organize-ext");
    fixture.write("piton.config.pi", CONFIG);
    fixture.write("spec/Other.pi", "export anchor Alpha:\n    a: 1\n");
    fixture.write(
        "spec/index.pi",
        "from ./Other.pi import Alpha\n\nexport anchor Root:\n    uses: {Alpha}\n",
    );
    let world = fixture.world();
    let uri = fixture.uri("spec/index.pi");
    let actions = features::code_actions(&world, &uri, Range::default()).expect("actions");
    let organize = actions
        .iter()
        .find_map(|action| match action {
            CodeActionOrCommand::CodeAction(action) if action.title == "Organize imports" => {
                Some(action)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{:?}", titles_of(&actions)));
    let edits = organize
        .edit
        .as_ref()
        .and_then(|edit| edit.changes.as_ref())
        .and_then(|changes| changes.get(&uri))
        .expect("edits");
    let text = std::fs::read_to_string(fixture.path("spec/index.pi")).expect("readable");
    assert!(apply(&text, edits).starts_with("from ./Other import Alpha\n"));
}

// ---------------------------------------------------------------------------
// Preview, workspace state
// ---------------------------------------------------------------------------

#[test]
fn a_construct_previews_in_every_renderer() {
    let fixture = Fixture::new("preview");
    fixture.write("piton.config.pi", CONFIG);
    fixture.write(
        "spec/index.pi",
        "export anchor Hello:\n    greeting: hi\n\nexport anchor Other:\n    x: 1\n",
    );
    let world = fixture.world();
    let uri = fixture.uri("spec/index.pi");
    let position = fixture.position("spec/index.pi", "Hello");

    let render = |renderer: Option<&str>| {
        let mut params = serde_json::json!({ "uri": uri, "position": position });
        if let Some(renderer) = renderer {
            params["renderer"] = serde_json::json!(renderer);
        }
        features::preview(&world, &params)
    };
    let json = render(None);
    assert_eq!(json["renderer"], "json", "{json}");
    let text = json["text"].as_str().expect("text");
    assert!(text.contains("\"greeting\": \"hi\""), "{text}");
    assert!(!text.contains("Other"), "only the anchor under the cursor: {text}");

    let yaml = render(Some("yaml"));
    assert!(yaml["text"].as_str().is_some_and(|text| text.contains("greeting: hi")), "{yaml}");
    let markdown = render(Some("markdown"));
    assert!(
        markdown["text"].as_str().is_some_and(|text| text.contains("# Hello")),
        "{markdown}"
    );
    assert!(markdown["path"].as_str().is_some_and(|path| path.ends_with("dist/index.md")));

    let whole = features::preview(&world, &serde_json::json!({ "uri": uri }));
    assert!(whole["text"].as_str().is_some_and(|text| text.contains("Other")), "{whole}");

    let unknown = render(Some("nonsense"));
    assert!(unknown.get("error").is_some(), "{unknown}");
}

#[test]
fn without_a_configuration_or_an_open_file_nothing_is_compiled() {
    let fixture = Fixture::new("no-config");
    fixture.write("spec/index.pi", "export anchor Lonely:\n    a: 1\n");
    let mut world = World::new(&fixture.dir);
    world.recompile(None);
    assert!(
        world.compilation.is_none(),
        "the workspace directory is not an entry point"
    );

    let path = fixture.path("spec/index.pi");
    world.set_document(path.clone(), "export anchor Lonely:\n    a: 1\n".into());
    world.recompile(None);
    assert!(world
        .compilation
        .as_ref()
        .is_some_and(|compilation| compilation.graph().id_for(&path).is_some()));
}

#[test]
fn an_unchanged_workspace_is_not_recompiled() {
    let fixture = Fixture::new("incremental");
    fixture.write("piton.config.pi", CONFIG);
    fixture.write("spec/index.pi", "export anchor Root:\n    a: 1\n");
    let mut world = fixture.world();
    assert!(!world.needs_compile());
    assert!(!world.refresh(None), "nothing changed");

    let path = fixture.path("spec/index.pi");
    world.set_document(path.clone(), "export anchor Root:\n    a: 2\n".into());
    assert!(world.needs_compile());
    assert!(world.refresh(Some(&path)));
    assert!(!world.refresh(Some(&path)), "compiled once per change");

    world.invalidate();
    assert!(world.needs_compile(), "a change on disk is a change");
}
