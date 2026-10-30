//! The editor features, held to exact answers.
//!
//! Each test writes a small workspace, puts the cursor somewhere with `$0`,
//! and checks the whole answer -- the tokens, the hover text, the edits --
//! rather than a fragment of it, so a change in behaviour shows up as a
//! failing line here and not as a surprise in an editor.

mod support;

use piton_lsp::features;
use piton_lsp::world::ClientCaps;
use support::{apply, edits_by_file, hint_label, Workspace};
use tower_lsp::lsp_types::*;

const BASE: &str = "\
export abstract anchor Base as base:
    name: base
    items:
        - zero
    title:: string
";

// ---------------------------------------------------------------------------
// Protocol surface
// ---------------------------------------------------------------------------

#[test]
fn the_capabilities_are_the_designed_ones() {
    let caps = piton_lsp::server_capabilities();
    assert!(caps.type_definition_provider.is_some());
    assert!(caps.document_highlight_provider.is_some());
    assert!(caps.document_range_formatting_provider.is_none());
    assert!(caps.document_link_provider.is_none());
    assert!(caps.signature_help_provider.is_none());
    assert!(caps.position_encoding.is_none(), "UTF-16, the default, is the only encoding");
    let Some(SemanticTokensServerCapabilities::SemanticTokensOptions(tokens)) =
        caps.semantic_tokens_provider
    else {
        panic!("semantic tokens")
    };
    assert_eq!(tokens.full, Some(SemanticTokensFullOptions::Bool(true)));
    assert!(tokens.range.is_none());
    let will_rename = caps
        .workspace
        .and_then(|workspace| workspace.file_operations)
        .and_then(|operations| operations.will_rename)
        .expect("willRename");
    assert_eq!(will_rename.filters[0].pattern.glob, "**/*.pi");
    let Some(TextDocumentSyncCapability::Options(sync)) = caps.text_document_sync else {
        panic!("sync options")
    };
    assert_eq!(sync.change, Some(TextDocumentSyncKind::FULL));
}

#[test]
fn only_a_change_elsewhere_refreshes_other_files() {
    use piton_lsp::{refreshes_after, Trigger};
    assert!(!refreshes_after(Trigger::Edit));
    assert!(refreshes_after(Trigger::Elsewhere));
}

#[test]
fn the_root_falls_back_to_the_root_uri() {
    #[allow(deprecated)]
    let params = InitializeParams {
        root_uri: Some(Url::parse("file:///tmp/project").expect("url")),
        ..Default::default()
    };
    assert_eq!(
        piton_lsp::workspace_root(&params),
        Some(std::path::PathBuf::from("/tmp/project"))
    );
}

// ---------------------------------------------------------------------------
// Semantic tokens
// ---------------------------------------------------------------------------

#[test]
fn semantic_tokens_name_what_resolves_and_nothing_else() {
    let ws = Workspace::new(&format!(
        "\
//- /spec/Base.pi
{BASE}
export helper: hello
//- /spec/index.pi
from ./Main export *
//- /spec/Main.pi
use ./Base
from ./Base import Base, helper H
// A comment that mentions Base
base Thing:
    title: Use Base, true and 42 here.
    count: 3
    total: {{this.count + 1}} and ${{H}} and @{{Base.name}}
    items:
        + {{super.items}}
$0"
    ));
    let tokens: Vec<String> = ws
        .tokens("spec/Main.pi")
        .into_iter()
        .map(|(line, column, text, kind, modifiers)| {
            format!("{line}:{column} {text} {kind} {}", modifiers.join(","))
        })
        .collect();
    assert_eq!(
        tokens,
        vec![
            "0:4 ./Base namespace ",
            "1:5 ./Base namespace ",
            "1:19 Base class abstract,exported,imported",
            "1:25 helper variable exported,imported",
            "1:32 H variable exported,imported",
            "3:0 base keyword imported",
            "3:5 Thing class declaration",
            "4:4 title property declaration",
            "5:4 count property declaration",
            "6:4 total property declaration",
            "6:17 count property ",
            "6:34 H variable exported,imported",
            "6:43 Base class abstract,exported,imported",
            "6:48 name property ",
            "7:4 items property declaration",
            "8:17 items property ",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

fn hover_at(spec: &str) -> Option<(String, String)> {
    Workspace::new(spec).hover_text()
}

#[test]
fn hover_shows_an_anchor_with_a_link_to_where_it_is_declared() {
    let ws = Workspace::new(&format!(
        "\
//- /spec/Base.pi
{BASE}
//- /spec/index.pi
use ./Base
from ./Base import Base

anchor Th$0ing extends Base:
    description: The first sentence. And more.
    title: Hi
"
    ));
    let (text, covered) = ws.hover_text().expect("hover");
    assert_eq!(covered, "Thing");
    let link = format!(
        "[`spec/index.pi:4`]({}#L4)",
        ws.uri("spec/index.pi")
    );
    assert!(
        text.starts_with(&format!(
            "```piton\nanchor Thing extends Base\n```\n\nThe first sentence.\n\nInherits: Base\n\nDeclared in {link} \u{b7} not exported\n\nResolves to (json):"
        )),
        "{text}"
    );
}

#[test]
fn hover_tells_overriding_from_implementing() {
    let spec = |cursor: &str| {
        format!(
            "\
//- /spec/Base.pi
{BASE}
//- /spec/index.pi
use ./Base
from ./Base import Base

export base Thing:
    {cursor}
"
        )
    };
    let (implements, _) = hover_at(&spec("ti$0tle: Hi")).expect("hover");
    assert!(implements.starts_with("`title` on `Thing`\n\nImplements `Base.title`\n\nConstrained to: string"), "{implements}");
    let (overrides, _) = hover_at(&spec("title: Hi\n    na$0me: other")).expect("hover");
    assert!(overrides.starts_with("`name` on `Thing`\n\nOverrides `Base.name`"), "{overrides}");
    let (required, _) = hover_at(&format!("//- /spec/index.pi\n{}", BASE.replace("title::", "ti$0tle::"))).expect("hover");
    assert!(required.contains("_Required: an implementing anchor must define it._"), "{required}");
}

#[test]
fn hover_explains_this_self_and_super() {
    let spec = |word: &str| {
        format!(
            "//- /spec/index.pi\nexport anchor A:\n    x: 1\nexport anchor B:\n    y: 2\nexport anchor C extends A, B:\n    z: {{{word}.x}}\n"
        )
    };
    for (word, expected) in [
        ("th$0is", "`this` is `C`, the anchor this expression is written in."),
        ("se$0lf", "`self` is the anchor being compiled: `C` or an anchor that extends it."),
        ("su$0per", "`super` is the merge of `A`, `B`; the right-most wins."),
    ] {
        let (text, covered) = hover_at(&spec(word)).expect("hover");
        assert_eq!(text, expected);
        assert_eq!(covered, word.replace("$0", ""));
    }
}

#[test]
fn hover_says_nothing_about_text() {
    for marked in [
        "    description: Use Base$0 here\n",
        "    // a comm$0ent\n    pass\n",
        "    count:$0 3\n",
        "    count: 4$02\n",
        "    v: a \\ {x$0} \\ b\n",
        "    v:\n        \\\\\\\n        lit$0eral\n        \\\\\\\n",
    ] {
        let spec = format!("//- /spec/index.pi\nexport anchor Base:\n{marked}");
        assert_eq!(hover_at(&spec), None, "{marked}");
    }
    assert_eq!(hover_at("//- /spec/index.pi\nexport anch$0or Base:\n    pass\n"), None);
}

#[test]
fn hover_on_an_import_name_after_its_path() {
    let (text, covered) = hover_at(&format!(
        "//- /spec/Base.pi\n{BASE}//- /spec/index.pi\nfrom ./Base import Ba$0se\n"
    ))
    .expect("hover");
    assert_eq!(covered, "Base");
    assert!(text.starts_with("```piton\nexport abstract anchor Base as base\n```"), "{text}");
}

#[test]
fn hover_on_a_composition_covers_its_operator() {
    let ws = Workspace::new(&format!(
        "//- /spec/Base.pi\n{BASE}//- /spec/index.pi\nuse ./Base\n\nexport base T:\n    title: t\n    items:\n        $0+ {{super.items}}\n"
    ));
    let (text, covered) = ws.hover_text().expect("hover");
    assert_eq!(covered, "+");
    assert!(text.starts_with("**Composition**"), "{text}");
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

#[test]
fn definition_answers_with_a_link_when_the_client_takes_one() {
    let caps = ClientCaps {
        link_support: true,
        ..Default::default()
    };
    let ws = Workspace::with_caps(
        &format!("//- /spec/Base.pi\n{BASE}//- /spec/index.pi\nfrom ./Base import B$0ase\n"),
        caps,
    );
    let Some(GotoDefinitionResponse::Link(links)) =
        features::definition(&ws.world, &ws.uri("spec/index.pi"), ws.position())
    else {
        panic!("a link")
    };
    let link = &links[0];
    assert_eq!(ws.slice("spec/index.pi", link.origin_selection_range.expect("origin")), "Base");
    assert_eq!(ws.slice("spec/Base.pi", link.target_selection_range), "Base");
    assert_eq!(
        ws.slice("spec/Base.pi", link.target_range),
        BASE.trim_end()
    );
}

#[test]
fn document_highlight_marks_writes_and_reads() {
    let ws = Workspace::new(
        "//- /spec/index.pi\nexport anchor A:\n    co$0unt: 1\n    twice: {this.count + this.count}\n",
    );
    let highlights =
        features::document_highlight(&ws.world, &ws.uri("spec/index.pi"), ws.position())
            .expect("highlights");
    let kinds: Vec<(u32, Option<DocumentHighlightKind>)> = highlights
        .iter()
        .map(|highlight| (highlight.range.start.line, highlight.kind))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (1, Some(DocumentHighlightKind::WRITE)),
            (2, Some(DocumentHighlightKind::READ)),
            (2, Some(DocumentHighlightKind::READ)),
        ]
    );
}

#[test]
fn type_definition_follows_an_inherited_constraint() {
    let ws = Workspace::new(
        "//- /spec/index.pi\nexport anchor Adapter:\n    pass\nexport abstract anchor Host:\n    adapters:: Adapter[]\nexport anchor Real extends Host:\n    adap$0ters:\n        - {Adapter}\n",
    );
    let Some(request::GotoTypeDefinitionResponse::Array(locations)) =
        features::type_definition(&ws.world, &ws.uri("spec/index.pi"), ws.position())
    else {
        panic!("locations")
    };
    let places: Vec<String> = locations.iter().map(|location| ws.place(location)).collect();
    assert_eq!(places, vec!["spec/index.pi:1"]);

    let builtin = Workspace::new("//- /spec/index.pi\nexport anchor A:\n    co$0unt:: number: 1\n");
    assert!(features::type_definition(&builtin.world, &builtin.uri("spec/index.pi"), builtin.position()).is_none());
}

// ---------------------------------------------------------------------------
// Rename
// ---------------------------------------------------------------------------

#[test]
fn renaming_an_imported_anchor_rewrites_the_name_and_not_the_path() {
    let ws = Workspace::new(&format!(
        "//- /spec/Base.pi\n{BASE}//- /spec/index.pi\nfrom ./Base import Base\n\nexport anchor T extends B$0ase:\n    title: t\n"
    ));
    let edit = features::rename(&ws.world, &ws.uri("spec/index.pi"), ws.position(), "Root")
        .expect("valid")
        .expect("an edit");
    let edited: Vec<(String, String)> = edits_by_file(&ws.dir, &edit)
        .into_iter()
        .map(|(file, edits)| {
            let text = ws.text(&file);
            (file, apply(&text, &edits))
        })
        .collect();
    assert_eq!(
        edited,
        vec![
            ("spec/Base.pi".to_string(), BASE.replace("anchor Base", "anchor Root")),
            (
                "spec/index.pi".to_string(),
                "from ./Base import Root\n\nexport anchor T extends Root:\n    title: t\n".to_string()
            ),
        ]
    );
}

#[test]
fn rename_refuses_what_it_cannot_do() {
    let ws = Workspace::new("//- /spec/index.pi\nexport anchor Th$0ing:\n    pass\nexport anchor Other:\n    pass\n");
    let uri = ws.uri("spec/index.pi");
    let position = ws.position();
    let error = |name: &str| features::rename(&ws.world, &uri, position, name).expect_err(name);
    assert!(error("1abc").contains("is not an anchor name"));
    assert!(error("null").contains("is not an anchor name"));
    assert_eq!(error("Other"), "`Other` is already defined in `".to_string() + &ws.path("spec/index.pi").display().to_string() + "`.");

    let keyword = Workspace::new("//- /spec/index.pi\nexport anchor K as k$0w:\n    pass\n");
    let message = features::rename(&keyword.world, &keyword.uri("spec/index.pi"), keyword.position(), "My-Kw")
        .expect_err("a bad keyword");
    assert!(message.contains("is not a keyword"), "{message}");

    let path = Workspace::new("//- /spec/index.pi\nuse ./Ot$0her\n//- /spec/Other.pi\npass_holder: 1\n");
    assert_eq!(
        features::prepare_rename(&path.world, &path.uri("spec/index.pi"), path.position()),
        Err("A module path is renamed by moving its file.".to_string())
    );

    let package = Workspace::new("//- /spec/index.pi\nuse @piton/belay\n\nexport sk$0ill Review:\n    description: Reviews.\n");
    let refused = features::prepare_rename(&package.world, &package.uri("spec/index.pi"), package.position())
        .expect_err("a package keyword");
    assert!(refused.ends_with("is declared in `@piton/belay` and cannot be renamed."), "{refused}");
}

#[test]
fn rename_sends_versioned_changes_when_the_client_takes_them() {
    let caps = ClientCaps {
        document_changes: true,
        ..Default::default()
    };
    let ws = Workspace::with_caps("//- /spec/index.pi\nexport anchor Th$0ing:\n    pass\n", caps);
    let edit = features::rename(&ws.world, &ws.uri("spec/index.pi"), ws.position(), "Renamed")
        .expect("valid")
        .expect("edit");
    let Some(DocumentChanges::Edits(changes)) = edit.document_changes else {
        panic!("document changes")
    };
    assert_eq!(changes[0].text_document.version, Some(7), "the open buffer's version");
    assert!(edit.changes.is_none());
}

#[test]
fn moving_a_file_rewrites_the_paths_that_name_it() {
    let ws = Workspace::new(
        "\
//- /spec/index.pi
from ./lib export *
//- /spec/lib/index.pi
from ./Type export *
//- /spec/lib/Type.pi
use ../Keywords.pi
export anchor Type:
    pass
//- /spec/Keywords.pi
export anchor Kw as kw:
    pass
//- /spec/Use.pi
from ./lib/Type import Type
from /lib/Type import Type T2
use @piton/belay
$0",
    );
    let rename = |from: &str, to: &str| {
        let edit = features::will_rename_files(
            &ws.world,
            &[FileRename {
                old_uri: ws.uri(from).to_string(),
                new_uri: ws.uri(to).to_string(),
            }],
        )
        .expect("an edit");
        edits_by_file(&ws.dir, &edit)
            .into_iter()
            .map(|(file, edits)| (file.clone(), apply(&ws.text(&file), &edits)))
            .collect::<Vec<_>>()
    };

    // `spec/types` sits beside `spec/lib`, so the moved file's own
    // `../Keywords.pi` still names the same file and is left as it is.
    let moved = rename("spec/lib/Type.pi", "spec/types/Type.pi");
    assert_eq!(
        moved,
        vec![
            ("spec/Use.pi".to_string(), "from ./types/Type import Type\nfrom /types/Type import Type T2\nuse @piton/belay\n".to_string()),
            ("spec/lib/index.pi".to_string(), "from ../types/Type export *\n".to_string()),
        ]
    );

    let folder = rename("spec/lib", "spec/kinds");
    assert!(folder.contains(&("spec/index.pi".to_string(), "from ./kinds export *\n".to_string())), "{folder:?}");
}

// ---------------------------------------------------------------------------
// Completion
// ---------------------------------------------------------------------------

const PAGE: &str = "//- /spec/Page.pi\nexport abstract anchor Page as page:\n    title:: string\n    path:: string\n    note: optional\n";

#[test]
fn a_body_offers_the_keys_it_does_not_have() {
    let ws = Workspace::new(&format!("{PAGE}//- /spec/index.pi\nuse ./Page\n\nexport page Home:\n    title: Home\n    $0\n"));
    assert_eq!(ws.labels(), vec!["note", "path"]);
    let items = ws.completion();
    let path = items.iter().find(|item| item.label == "path").expect("path");
    assert_eq!(path.sort_text.as_deref(), Some("0path"), "required first");
    let empty = Workspace::new(&format!("{PAGE}//- /spec/index.pi\nuse ./Page\n\nexport page Home:\n    $0\n"));
    assert_eq!(empty.labels(), vec!["note", "pass", "path", "title"]);
}

#[test]
fn a_key_completed_before_its_colon_keeps_one_colon() {
    let ws = Workspace::new(&format!("{PAGE}//- /spec/index.pi\nuse ./Page\n\nexport page Home:\n    pa$0: x\n"));
    let item = ws.completion().into_iter().find(|item| item.label == "path").expect("path");
    let Some(CompletionTextEdit::Edit(edit)) = item.text_edit else {
        panic!("an edit")
    };
    assert_eq!(edit.new_text, "path");
}

#[test]
fn insert_and_replace_ranges_when_the_client_takes_them() {
    let caps = ClientCaps {
        insert_replace: true,
        ..Default::default()
    };
    let ws = Workspace::with_caps(&format!("{PAGE}//- /spec/index.pi\nuse ./Page\n\nexport page Home:\n    pa$0th: x\n"), caps);
    let item = ws.completion().into_iter().find(|item| item.label == "path").expect("path");
    let Some(CompletionTextEdit::InsertAndReplace(edit)) = item.text_edit else {
        panic!("insert and replace")
    };
    assert_eq!(ws.slice("spec/index.pi", edit.insert), "pa");
    assert_eq!(ws.slice("spec/index.pi", edit.replace), "path");
}

#[test]
fn resolving_an_item_from_an_older_compilation_adds_nothing() {
    let mut ws = Workspace::new(&format!("{PAGE}//- /spec/index.pi\nuse ./Page\n\nexport page Home:\n    $0\n"));
    let item = ws.completion().into_iter().find(|item| item.label == "path").expect("path");
    let current = features::resolve_completion(&ws.world, item.clone());
    assert!(current.documentation.is_some());
    let path = ws.path("spec/index.pi");
    ws.world.invalidate();
    ws.world.recompile(Some(&path));
    let stale = features::resolve_completion(&ws.world, item);
    assert!(stale.documentation.is_none());
}

#[test]
fn nothing_completes_inside_an_inline_escape() {
    let ws = Workspace::new("//- /spec/index.pi\nexport anchor A:\n    helper: x\n    v: see \\ {hel$0\n");
    assert!(ws.labels().is_empty(), "{:?}", ws.labels());
}

// ---------------------------------------------------------------------------
// Formatting, folding, selection
// ---------------------------------------------------------------------------

#[test]
fn formatting_sends_only_the_changed_lines() {
    let text = "export anchor A:\n    a: 1\n    b: 2\n    c: 3\n    d: 4\n    e: 5\n  f: 6\n";
    let path = std::path::Path::new("x.pi");
    let edits = features::formatting(text, path);
    assert_eq!(edits.len(), 1, "{edits:?}");
    assert_eq!(edits[0].range.start.line, 6);
    assert_eq!(edits[0].range.end.line, 7);
    assert_eq!(apply(text, &edits), piton_syntax::format::autoformat(text, path));
    assert!(features::formatting(&piton_syntax::format::autoformat(text, path), path).is_empty());
}

#[test]
fn folding_is_whole_lines_of_structure() {
    let text = "\
use ./a
use ./b

// one
// two
export anchor A:
    single: x
    block:
        - one
        - two
    example:
        ```piton
        \\\\\\
        anchor X:
            pass
        \\\\\\
        ```

broken extends
    title: Hi
";
    let ranges: Vec<(u32, u32, Option<FoldingRangeKind>)> =
        features::folding(text, std::path::Path::new("x.pi"))
            .into_iter()
            .map(|range| (range.start_line, range.end_line, range.kind))
            .collect();
    assert_eq!(
        ranges,
        vec![
            (0, 1, Some(FoldingRangeKind::Imports)),
            (3, 4, Some(FoldingRangeKind::Comment)),
            (5, 16, None),
            (7, 9, None),
            (10, 16, None),
            (11, 16, None),
            (18, 19, None),
        ]
    );
}

#[test]
fn selection_grows_from_a_name_to_the_file() {
    let text = "export anchor A:\n    count: 1\n    next: {this.count + 1}\n";
    let offset = text.find("count +").expect("count") + 2;
    let position = piton_lsp::convert::offset_to_position(text, offset);
    let ranges = features::selection_ranges(text, std::path::Path::new("x.pi"), &[position]);
    let mut chain = Vec::new();
    let mut current = Some(Box::new(ranges[0].clone()));
    while let Some(range) = current {
        let start = piton_lsp::convert::position_to_offset(text, range.range.start);
        let end = piton_lsp::convert::position_to_offset(text, range.range.end);
        chain.push(text[start..end].to_string());
        current = range.parent;
    }
    assert_eq!(
        chain,
        vec![
            "count",
            "this.count",
            "this.count + 1",
            "{this.count + 1}",
            "next: {this.count + 1}",
            "count: 1\n    next: {this.count + 1}",
            text.trim_end(),
        ]
    );
}

// ---------------------------------------------------------------------------
// Symbols
// ---------------------------------------------------------------------------

#[test]
fn workspace_symbols_rank_by_how_well_they_match() {
    assert_eq!(features::match_tier("Thing", "thing"), Some(0));
    assert_eq!(features::match_tier("Thing", "th"), Some(1));
    assert_eq!(features::match_tier("Path", "th"), Some(2));
    assert_eq!(features::match_tier("Thing", "thng"), Some(3));
    assert_eq!(features::match_tier("Thing", "x"), None);

    let ws = Workspace::new("//- /spec/index.pi\nexport anchor Path:\n    pass\nexport anchor Thing:\n    pass\nexport anchor Th:\n    pass\n");
    let found = features::workspace_symbols(&ws.world, "th").expect("symbols");
    let names: Vec<&str> = found.iter().map(|symbol| symbol.name.as_str()).collect();
    assert_eq!(names, vec!["Th", "Thing", "Path"]);
    assert_eq!(found[0].container_name.as_deref(), Some("spec/index"));
}

#[test]
fn document_symbols_nest_dictionary_keys() {
    let ws = Workspace::new("//- /spec/index.pi\nexport anchor A:\n    config:: dictionary:\n        depth: 1\n    items:\n        - one\n$0");
    let Some(DocumentSymbolResponse::Nested(symbols)) =
        features::document_symbols(&ws.world, &ws.uri("spec/index.pi"))
    else {
        panic!("nested symbols")
    };
    let config = &symbols[0].children.as_ref().expect("properties")[0];
    assert_eq!(config.name, "config");
    assert_eq!(config.detail.as_deref(), Some("dictionary"));
    let depth = &config.children.as_ref().expect("keys")[0];
    assert_eq!((depth.name.as_str(), depth.kind), ("depth", SymbolKind::FIELD));
    let items = &symbols[0].children.as_ref().expect("properties")[1];
    assert!(items.children.as_ref().is_none_or(Vec::is_empty), "list items are not symbols");
}

// ---------------------------------------------------------------------------
// Inlay hints and lenses
// ---------------------------------------------------------------------------

#[test]
fn inlay_hints_say_only_what_the_source_does_not() {
    let ws = Workspace::new(
        "\
//- /spec/Other.pi
export anchor Other:
    pass
//- /spec/index.pi
export anchor Root:
    p: root
export anchor Foo extends Root:
    own: 1
    items:
        - a
export anchor Child extends Foo:
    x: {1 + 2}
    y: ${own}
    c:: number: {1}
    t: {Foo}
    l: {[\"a\"]}
    m: {[1, \"a\"]}
    s: {super.items}
    o: {this.x}
    i: {this.own}
    b: {Foo.p}
    z: {self.x}
$0",
    );
    let hints: Vec<String> = ws
        .inlay_hints("spec/index.pi")
        .iter()
        .map(|hint| {
            format!(
                "{}:{} {} {}",
                hint.position.line,
                hint.position.character,
                hint_label(hint),
                if hint.text_edits.is_some() { "edit" } else { "-" }
            )
        })
        .collect();
    assert_eq!(
        hints,
        vec![
            "7:5 :: number edit",
            "10:5 :: Foo edit",
            "11:5 :: string[] edit",
            "12:5 :: list edit",
            // `super` names no one anchor, so whoever supplies it is news.
            "13:5 :: string[] edit",
            "13:19 \u{2190} Foo -",
            // An inherited value read through `this` names its supplier; the
            // anchor's own does not.
            "14:5 :: number edit",
            "15:5 :: number edit",
            "15:16 \u{2190} Foo -",
            // Read through an anchor whose base supplies it.
            "16:5 :: string edit",
            "16:13 \u{2190} Root -",
            // `self` is whichever anchor is compiled, so it names no supplier.
            "17:5 :: number edit",
        ]
    );
}

#[test]
fn an_override_lens_is_only_for_a_replaced_value() {
    let ws = Workspace::new(&format!(
        "//- /spec/Base.pi\n{BASE}//- /spec/index.pi\nuse ./Base\n\nexport base T:\n    title: Hi\n    name: other\n"
    ));
    let titles: Vec<String> = features::code_lenses(&ws.world, &ws.uri("spec/index.pi"))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|lens| lens.command.map(|command| command.title))
        .filter(|title| title.starts_with("overrides"))
        .collect();
    assert_eq!(titles, vec!["overrides Base.name"]);
}

#[test]
fn diagnostics_carry_the_version_they_were_computed_from() {
    let ws = Workspace::new("//- /spec/index.pi\nexport anchor A:\n    v: {Missing}\n$0");
    assert_eq!(ws.world.compiled_version(&ws.path("spec/index.pi")), Some(7));
    assert_eq!(ws.world.compiled_version(&ws.path("piton.config.pi")), None);
}
