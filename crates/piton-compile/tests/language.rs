//! The language semantics, checked against the examples in the specification.

use std::path::{Path, PathBuf};

use piton_compile::{Compilation, Project};
use piton_core::{AnchorView, Value};
use piton_emit::{json, markdown};

struct Sandbox {
    dir: PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let dir = std::env::temp_dir().join(format!(
            "piton-lang-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        Sandbox { dir }
    }

    fn file(&self, relative: &str, contents: &str) -> &Sandbox {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, contents).expect("write");
        self
    }

    fn compile(&self, entry: &str) -> Compilation {
        let path = self.dir.join(entry);
        let mut project = Project::for_file(&path);
        project.source_root = self.dir.clone();
        project.root = self.dir.clone();
        Compilation::build(project)
    }
}

/// Compiles a single file and returns the named anchor as JSON, which is the
/// representation the specification uses for its worked examples.
fn compile_to_json(source: &str, anchor: &str) -> String {
    let sandbox = Sandbox::new(anchor);
    sandbox.file("main.pi", source);
    let compilation = sandbox.compile("main.pi");
    let errors: Vec<String> = compilation
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    let id = compilation.find_anchor(anchor).expect("anchor");
    json::value(&Value::Anchor(id), &compilation)
}

fn property(source: &str, anchor: &str, name: &str) -> Value {
    let sandbox = Sandbox::new(&format!("{anchor}-{name}"));
    sandbox.file("main.pi", source);
    let compilation = sandbox.compile("main.pi");
    let errors: Vec<String> = compilation
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| format!("{} at {}: {}", d.code, d.span.start, d.message))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    let id = compilation.find_anchor(anchor).expect("anchor");
    compilation
        .properties(id)
        .get(name)
        .cloned()
        .unwrap_or_else(|| panic!("no property `{name}`"))
}

fn text_of(value: &Value) -> String {
    match value {
        Value::Str(text) => text.to_string(),
        other => panic!("expected a string, got {other:?}"),
    }
}

fn list_of(value: &Value) -> Vec<String> {
    match value {
        Value::List(items) => items
            .iter()
            .map(|item| match item {
                Value::Str(text) => text.to_string(),
                other => format!("{other:?}"),
            })
            .collect(),
        other => panic!("expected a list, got {other:?}"),
    }
}

// -- inheritance ----------------------------------------------------------

#[test]
fn multiple_bases_resolve_left_to_right() {
    let json = compile_to_json(
        r#"anchor FirstBaseAnchor:
    firstBaseAnchorProperty: Hello from the First Base Anchor
    description: Description from FirstBaseAnchor

anchor SecondBaseAnchor:
    secondBaseAnchorProperty: Hello from the Second Base Anchor
    description: Description from SecondBaseAnchor

export anchor ChildAnchor extends FirstBaseAnchor, SecondBaseAnchor:
    childAnchorProperty: Hello from Child Anchor
"#,
        "ChildAnchor",
    );
    assert_eq!(
        json,
        r#"{
  "firstBaseAnchorProperty": "Hello from the First Base Anchor",
  "description": "Description from SecondBaseAnchor",
  "secondBaseAnchorProperty": "Hello from the Second Base Anchor",
  "childAnchorProperty": "Hello from Child Anchor"
}"#
    );
}

#[test]
fn super_reads_the_right_most_base() {
    let value = property(
        r#"anchor BaseAnchor:
    description: Description from BaseAnchor

export anchor ChildAnchor extends BaseAnchor:
    description:
        ${super.description} and Description from ChildAnchor
"#,
        "ChildAnchor",
        "description",
    );
    assert_eq!(
        text_of(&value),
        "Description from BaseAnchor and Description from ChildAnchor"
    );
}

#[test]
fn self_travels_the_chain_while_this_stays_pinned() {
    let source = r#"anchor Base:
    name: Base Anchor
    baseDescription: This is ${this.name}

anchor MidChild extends Base:
    name: MidChild Anchor
    baseDescription: Override on baseDescription ${this.name}
    childDescription: This is ${self.name}

export anchor FinalChild extends MidChild:
    name: FinalChild Anchor
"#;
    assert_eq!(
        compile_to_json(source, "FinalChild"),
        r#"{
  "name": "FinalChild Anchor",
  "baseDescription": "Override on baseDescription MidChild Anchor",
  "childDescription": "This is FinalChild Anchor"
}"#
    );
}

#[test]
fn self_alone_resolves_to_the_derived_anchor_name() {
    let source = r#"anchor Base:
    label: called ${self}

export anchor Child extends Base:
    extra: 1
"#;
    assert_eq!(text_of(&property(source, "Child", "label")), "called Child");
}

// -- super on lists -------------------------------------------------------

#[test]
fn merging_super_first_appends_the_new_items() {
    let source = r#"anchor BaseAnchor:
    items:
        - A
        - B
        - C

export anchor ChildAnchor extends BaseAnchor:
    items:
        + {super.items}
        - D
        - E
        - F
"#;
    assert_eq!(
        list_of(&property(source, "ChildAnchor", "items")),
        vec!["A", "B", "C", "D", "E", "F"]
    );
}

#[test]
fn merging_super_last_deduplicates_by_keeping_the_later_position() {
    let source = r#"anchor BaseAnchor:
    items:
        - A
        - B
        - C

export anchor ChildAnchor extends BaseAnchor:
    items:
        - A
        - B
        - C
        - D
        + {super.items}
"#;
    assert_eq!(
        list_of(&property(source, "ChildAnchor", "items")),
        vec!["D", "A", "B", "C"]
    );
}

#[test]
fn the_duplicating_merge_keeps_every_item() {
    let source = r#"anchor BaseAnchor:
    items:
        - A
        - B
        - C

export anchor ChildAnchor extends BaseAnchor:
    items:
        - A
        - B
        - C
        - D
        ++ {super.items}
"#;
    assert_eq!(
        list_of(&property(source, "ChildAnchor", "items")),
        vec!["A", "B", "C", "D", "A", "B", "C"]
    );
}

// -- user keywords --------------------------------------------------------

#[test]
fn a_user_keyword_is_the_left_most_base() {
    let source = r#"anchor MyAnchor as my-anchor:
    description: This is a description of my anchor

anchor OtherBase:
    description: Other Base

export my-anchor ChildAnchor extends OtherBase:
    extra: 1
"#;
    // The keyword contributes the left-most base, so `OtherBase` wins.
    assert_eq!(
        text_of(&property(source, "ChildAnchor", "description")),
        "Other Base"
    );
}

#[test]
fn a_keyword_declaration_equals_the_extends_form() {
    let keyword = compile_to_json(
        r#"anchor MyAnchor as my-anchor:
    description: This is a description of my anchor

export my-anchor ChildAnchor:
    description:
        + {super.description}
        My additional description
"#,
        "ChildAnchor",
    );
    let extends = compile_to_json(
        r#"anchor MyAnchor:
    description: This is a description of my anchor

export anchor ChildAnchor extends MyAnchor:
    description:
        + {super.description}
        My additional description
"#,
        "ChildAnchor",
    );
    assert_eq!(keyword, extends);
}

// -- expressions ----------------------------------------------------------

#[test]
fn arithmetic_evaluates_inside_braces_only() {
    let source = r#"export anchor A:
    evaluated: {1 + 2}
    literal: 1 + 2
    precedence: {1 + 2 * 3}
    parenthesized: {(1 + 2) * 3}
    subtraction: {3.14 - 3.14}
"#;
    assert_eq!(property(source, "A", "evaluated"), Value::Number(3.0));
    assert_eq!(text_of(&property(source, "A", "literal")), "1 + 2");
    assert_eq!(property(source, "A", "precedence"), Value::Number(7.0));
    assert_eq!(property(source, "A", "parenthesized"), Value::Number(9.0));
    assert_eq!(property(source, "A", "subtraction"), Value::Number(0.0));
}

#[test]
fn variables_resolve_forward_and_across_collections() {
    let source = r#"myList: [1, 2, 3]

myDictionary:
    list: {myList}

newList: {myDictionary.list + [4, 5, 6]}

export anchor A:
    result: {newList}
"#;
    let value = property(source, "A", "result");
    let Value::List(items) = value else {
        panic!("expected a list")
    };
    let numbers: Vec<f64> = items
        .iter()
        .map(|item| match item {
            Value::Number(n) => *n,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(numbers, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
}

#[test]
fn mixed_type_addition_concatenates_as_text() {
    let source = "export anchor A:\n    joined: {2 + \"Hello\"}\n";
    assert_eq!(text_of(&property(source, "A", "joined")), "2Hello");
}

#[test]
fn the_numeric_cast_converts_whole_strings_only() {
    let source = "export anchor A:\n    n: #{\"42\"}\n    m: #{1 + 1}\n";
    assert_eq!(property(source, "A", "n"), Value::Number(42.0));
    assert_eq!(property(source, "A", "m"), Value::Number(2.0));
}

#[test]
fn a_numeric_cast_of_prose_is_an_error() {
    let sandbox = Sandbox::new("numeric-error");
    sandbox.file("main.pi", "export anchor A:\n    n: #{\"42 things\"}\n");
    let compilation = sandbox.compile("main.pi");
    assert!(compilation
        .diagnostics
        .iter()
        .any(|d| d.code == "not-a-number"));
}

#[test]
fn the_ternary_evaluates_only_the_selected_branch() {
    // Bare words inside an expression are symbols, so the branches are quoted.
    let source = r#"export anchor A:
    taken: {true ? "yes" : "no"}
    other: {false ? "yes" : "no"}
    chained: {false ? "one" : true ? "two" : "three"}
"#;
    assert_eq!(text_of(&property(source, "A", "taken")), "yes");
    assert_eq!(text_of(&property(source, "A", "other")), "no");
    assert_eq!(text_of(&property(source, "A", "chained")), "two");
}

#[test]
fn logical_and_comparison_operators_produce_booleans() {
    let source = r#"export anchor A:
    and: {true && false}
    or: {true || false}
    not: {!true}
    equal: {1 == 1}
    greater: {2 > 1}
    lessOrEqual: {2 <= 1}
"#;
    assert_eq!(property(source, "A", "and"), Value::Bool(false));
    assert_eq!(property(source, "A", "or"), Value::Bool(true));
    assert_eq!(property(source, "A", "not"), Value::Bool(false));
    assert_eq!(property(source, "A", "equal"), Value::Bool(true));
    assert_eq!(property(source, "A", "greater"), Value::Bool(true));
    assert_eq!(property(source, "A", "lessOrEqual"), Value::Bool(false));
}

// -- types and coercion ---------------------------------------------------

#[test]
fn constraints_are_tried_left_to_right() {
    let source = r#"export anchor A:
    numberFirst:: number:: string: 42
    stringFirst:: string:: number: 42
    quotedFallsThrough:: boolean:: number:: string: "false"
    unquotedBoolean:: boolean:: string: false
"#;
    assert_eq!(property(source, "A", "numberFirst"), Value::Number(42.0));
    assert_eq!(text_of(&property(source, "A", "stringFirst")), "42");
    assert_eq!(
        text_of(&property(source, "A", "quotedFallsThrough")),
        "false"
    );
    assert_eq!(property(source, "A", "unquotedBoolean"), Value::Bool(false));
}

#[test]
fn a_value_no_constraint_accepts_is_an_error() {
    let sandbox = Sandbox::new("coercion-error");
    sandbox.file("main.pi", "export anchor A:\n    v:: number: Hello\n");
    let compilation = sandbox.compile("main.pi");
    assert!(compilation
        .diagnostics
        .iter()
        .any(|d| d.code == "type-mismatch"));
}

#[test]
fn special_constraints_classify_by_shape() {
    let source = r#"export anchor A:
    a:: complex: [1, 2, 3]
    c:: simple: 1
    d:: simple: Hello, World
    e:: simple: false
    g:: any: 1
"#;
    assert!(property(source, "A", "a").is_complex());
    assert!(property(source, "A", "c").is_simple());
    assert!(property(source, "A", "d").is_simple());
    assert_eq!(property(source, "A", "e"), Value::Bool(false));
    assert_eq!(property(source, "A", "g"), Value::Number(1.0));
}

#[test]
fn a_list_constraint_checks_every_element() {
    let sandbox = Sandbox::new("list-constraint");
    sandbox.file(
        "main.pi",
        "export anchor A:\n    ok:: string[]: [foo, bar]\n    bad:: number[]: [1, two]\n",
    );
    let compilation = sandbox.compile("main.pi");
    let codes: Vec<&str> = compilation
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.code.as_str())
        .collect();
    assert_eq!(codes, vec!["type-mismatch"], "only the bad list should fail");
}

#[test]
fn inference_follows_the_documented_table() {
    let source = r#"export anchor A:
    boolean: true
    prose: true story
    number: 42
    numberish: 42 things
    nothing: null
    big: 1_200_000.00
    decimal: 0.14
"#;
    assert_eq!(property(source, "A", "boolean"), Value::Bool(true));
    assert_eq!(text_of(&property(source, "A", "prose")), "true story");
    assert_eq!(property(source, "A", "number"), Value::Number(42.0));
    assert_eq!(text_of(&property(source, "A", "numberish")), "42 things");
    assert_eq!(property(source, "A", "nothing"), Value::Null);
    assert_eq!(property(source, "A", "big"), Value::Number(1_200_000.0));
    assert_eq!(property(source, "A", "decimal"), Value::Number(0.14));
}

// -- abstracts ------------------------------------------------------------

#[test]
fn a_concrete_anchor_must_implement_every_abstract_slot() {
    let sandbox = Sandbox::new("abstract");
    sandbox.file(
        "main.pi",
        "abstract anchor Shape:\n    description:: string\n\nexport anchor Missing extends Shape:\n    other: 1\n",
    );
    let compilation = sandbox.compile("main.pi");
    let found = compilation
        .diagnostics
        .iter()
        .find(|d| d.code == "unimplemented-property")
        .expect("diagnostic");
    assert!(found.message.contains("does not define `description`"));
    assert!(!found.labels.is_empty(), "the declaring anchor is pointed at");
}

#[test]
fn extends_constraints_accept_anything_in_the_chain() {
    let source = r#"abstract anchor A:
    description:: string

abstract anchor B extends A:
    name:: string

anchor Direct extends A:
    description: direct

anchor Deep extends B:
    description: deep
    name: deep

export anchor C:
    listOfA:: A[]: [{Direct}]
    listOfExtendsA:: extends A[]: [{Direct}, {Deep}]
"#;
    let sandbox = Sandbox::new("extends-constraint");
    sandbox.file("main.pi", source);
    let compilation = sandbox.compile("main.pi");
    let errors: Vec<&str> = compilation
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.message.as_str())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn a_bare_anchor_constraint_rejects_an_indirect_implementer() {
    let sandbox = Sandbox::new("direct-only");
    sandbox.file(
        "main.pi",
        r#"abstract anchor A:
    description:: string

abstract anchor B extends A:
    name:: string

anchor Deep extends B:
    description: deep
    name: deep

export anchor C:
    listOfA:: A[]: [{Deep}]
"#,
    );
    let compilation = sandbox.compile("main.pi");
    assert!(
        compilation
            .diagnostics
            .iter()
            .any(|d| d.code == "type-mismatch"),
        "`A[]` takes direct implementers; `extends A[]` is the wider form"
    );
}

#[test]
fn implementing_two_abstracts_is_rejected() {
    let sandbox = Sandbox::new("two-abstracts");
    sandbox.file(
        "main.pi",
        "abstract anchor A:\n    a:: string\n\nabstract anchor B:\n    b:: string\n\nexport anchor C extends A, B:\n    a: one\n    b: two\n",
    );
    let compilation = sandbox.compile("main.pi");
    assert!(compilation
        .diagnostics
        .iter()
        .any(|d| d.code == "multiple-abstract-bases"));
}

// -- collections and prose ------------------------------------------------

#[test]
fn a_mixed_block_becomes_an_implicit_list_with_addressable_keys() {
    let source = r#"export anchor A:
    combined:
        This is a string

        - This
        - Is
        - A
        - List

        nestedDictionary:
            deeplyNestedDictionary: This is a string

    reachedThroughTheKey: {this.combined.nestedDictionary.deeplyNestedDictionary}
"#;
    let json = compile_to_json(source, "A");
    assert!(
        json.contains(
            r#""combined": [
    "This is a string",
    [
      "This",
      "Is",
      "A",
      "List"
    ],
    {
      "nestedDictionary": {
        "deeplyNestedDictionary": "This is a string"
      }
    }
  ]"#
        ),
        "{json}"
    );
    assert_eq!(
        text_of(&property(source, "A", "reachedThroughTheKey")),
        "This is a string"
    );
}

#[test]
fn nested_lists_flatten_the_way_the_bracket_form_does() {
    let block = compile_to_json(
        "export anchor A:\n    nested:\n        - Level 1\n            - Level 2\n                - Level 3\n",
        "A",
    );
    let inline = compile_to_json(
        "export anchor A:\n    nested: [Level 1, [Level 2, [Level 3]]]\n",
        "A",
    );
    assert_eq!(block, inline);
}

#[test]
fn a_blank_line_is_an_explicit_line_break() {
    let source = r#"export anchor A:
    text:
        This broken string is not considered
        a line break.

        While this is on a new line because there was a blank line above.
"#;
    assert_eq!(
        text_of(&property(source, "A", "text")),
        "This broken string is not considered a line break.\nWhile this is on a new line because there was a blank line above."
    );
}

#[test]
fn fenced_blocks_are_verbatim() {
    let source = "export anchor A:\n    text:\n        before\n\n        ```piton\n        // not a comment\n        key: not a property\n        ```\n";
    assert_eq!(
        text_of(&property(source, "A", "text")),
        "before\n```piton\n// not a comment\nkey: not a property\n```"
    );
}

#[test]
fn lists_reject_member_access() {
    let sandbox = Sandbox::new("list-access");
    sandbox.file(
        "main.pi",
        "myList: [1, 2, 3]\n\nexport anchor A:\n    v: {myList.first}\n",
    );
    let compilation = sandbox.compile("main.pi");
    assert!(compilation
        .diagnostics
        .iter()
        .any(|d| d.code == "list-access"));
}

// -- modules --------------------------------------------------------------

#[test]
fn circular_imports_are_allowed() {
    let sandbox = Sandbox::new("circular");
    sandbox
        .file(
            "a.pi",
            "from ./b import B\n\nexport anchor A:\n    talksAbout: This anchor talks about ${B}\n",
        )
        .file(
            "b.pi",
            "from ./a import A\n\nexport anchor B:\n    talksAbout: This anchor talks about ${A}\n",
        );
    let compilation = sandbox.compile("a.pi");
    let errors: Vec<&str> = compilation
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.message.as_str())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    let a = compilation.find_anchor("A").expect("A");
    assert_eq!(
        text_of(compilation.properties(a).get("talksAbout").expect("value")),
        "This anchor talks about B"
    );
}

#[test]
fn a_circular_value_is_an_error() {
    let sandbox = Sandbox::new("circular-value");
    sandbox.file(
        "main.pi",
        "export anchor A:\n    x: {B.y}\n\nexport anchor B:\n    y: {A.x}\n",
    );
    let compilation = sandbox.compile("main.pi");
    assert!(compilation
        .diagnostics
        .iter()
        .any(|d| d.code == "cyclic-value"));
}

#[test]
fn a_directory_resolves_to_its_index() {
    let sandbox = Sandbox::new("module");
    sandbox
        .file("lib/MyAnchor.pi", "export anchor MyAnchor:\n    v: 1\n")
        .file("lib/index.pi", "from ./MyAnchor export *\n")
        .file("main.pi", "from ./lib import MyAnchor\n\nexport anchor Uses:\n    got: {MyAnchor.v}\n");
    let compilation = sandbox.compile("main.pi");
    let errors: Vec<&str> = compilation
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.message.as_str())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    let uses = compilation.find_anchor("Uses").expect("Uses");
    assert_eq!(
        compilation.properties(uses).get("got"),
        Some(&Value::Number(1.0))
    );
}

#[test]
fn an_alias_replaces_the_original_binding() {
    let sandbox = Sandbox::new("alias");
    sandbox
        .file("first.pi", "export pi: 3.14\n")
        .file("main.pi", "from ./first import pi SliceOf\n\nexport anchor A:\n    v: {SliceOf}\n");
    let compilation = sandbox.compile("main.pi");
    let a = compilation.find_anchor("A").expect("A");
    assert_eq!(
        compilation.properties(a).get("v"),
        Some(&Value::Number(3.14))
    );

    // `pi` itself is not in scope under its original name.
    let sandbox = Sandbox::new("alias-original");
    sandbox
        .file("first.pi", "export pi: 3.14\n")
        .file("main.pi", "from ./first import pi SliceOf\n\nexport anchor A:\n    v: {pi}\n");
    let compilation = sandbox.compile("main.pi");
    assert!(compilation
        .diagnostics
        .iter()
        .any(|d| d.code == "unresolved-symbol"));
}

#[test]
fn importing_something_unexported_is_reported_with_a_suggestion() {
    let sandbox = Sandbox::new("unexported");
    sandbox
        .file("first.pi", "export anchor MyAnchor:\n    v: 1\nmyVariable: 42\n")
        .file("main.pi", "from ./first import myVariable\n");
    let compilation = sandbox.compile("main.pi");
    assert!(compilation
        .diagnostics
        .iter()
        .any(|d| d.code == "unresolved-import"));

    let sandbox = Sandbox::new("typo");
    sandbox
        .file("first.pi", "export anchor MyAnchor:\n    v: 1\n")
        .file("main.pi", "from ./first import MyAnchr\n");
    let compilation = sandbox.compile("main.pi");
    let found = compilation
        .diagnostics
        .iter()
        .find(|d| d.code == "unresolved-import")
        .expect("diagnostic");
    assert_eq!(found.help.as_deref(), Some("did you mean `MyAnchor`?"));
}

#[test]
fn use_brings_in_keywords_but_not_symbols() {
    let sandbox = Sandbox::new("use");
    sandbox
        .file(
            "keywords.pi",
            "export anchor Custom as my-custom-keyword:\n    description: base\n",
        )
        .file(
            "main.pi",
            "use ./keywords\n\nexport my-custom-keyword Wow:\n    extra: amazing\n",
        );
    let compilation = sandbox.compile("main.pi");
    let errors: Vec<&str> = compilation
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.message.as_str())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");

    // The keyword's anchor is not bound as a symbol.
    let sandbox = Sandbox::new("use-symbol");
    sandbox
        .file(
            "keywords.pi",
            "export anchor Custom as my-custom-keyword:\n    description: base\n",
        )
        .file("main.pi", "use ./keywords\n\nexport anchor A:\n    v: {Custom}\n");
    let compilation = sandbox.compile("main.pi");
    assert!(compilation
        .diagnostics
        .iter()
        .any(|d| d.code == "unresolved-symbol"));
}

#[test]
fn an_unknown_keyword_says_what_to_do() {
    let sandbox = Sandbox::new("unknown-keyword");
    sandbox.file("main.pi", "export my-keyword Thing:\n    v: 1\n");
    let compilation = sandbox.compile("main.pi");
    let found = compilation
        .diagnostics
        .iter()
        .find(|d| d.code == "unknown-keyword")
        .expect("diagnostic");
    assert!(found.help.as_deref().unwrap_or_default().contains("use"));
}

// -- escaping and references ---------------------------------------------

#[test]
fn escapes_keep_expression_syntax_literal() {
    let source = r#"export anchor A:
    description:
        So for example \\ \ {1 + 2 + 3} \ \\ would become \ { 1 + 2 + 3 } \.
"#;
    assert_eq!(
        text_of(&property(source, "A", "description")),
        r"So for example \ {1 + 2 + 3} \ would become { 1 + 2 + 3 }."
    );
}

#[test]
fn a_reference_keeps_identity_while_a_string_cast_takes_the_name() {
    let source = r#"anchor Target:
    v: 1

export anchor A:
    asReference: @{Target}
    asString: ${Target}
    asValue: {Target}
"#;
    let reference = property(source, "A", "asReference");
    assert!(matches!(reference, Value::Reference(_)));
    assert_eq!(text_of(&property(source, "A", "asString")), "Target");
    assert!(matches!(property(source, "A", "asValue"), Value::Anchor(_)));
}

#[test]
fn a_reference_renders_as_a_link_and_a_value_renders_inline() {
    let sandbox = Sandbox::new("render");
    sandbox.file(
        "main.pi",
        "anchor Target:\n    v: 1\n\nexport anchor A:\n    link: @{Target}\n    inline: {Target}\n",
    );
    let compilation = sandbox.compile("main.pi");
    let a = compilation.find_anchor("A").expect("A");
    let links = markdown::MirroredLinks {
        from_directory: &sandbox.dir,
        source_root: &sandbox.dir,
        anchors: &compilation,
    };
    let context = markdown::Context {
        anchors: &compilation,
        links: &links,
    };
    let rendered = markdown::document(a, &context);
    assert!(rendered.contains("[Target](Target.md)"), "{rendered}");
    assert!(rendered.contains("## Inline\n\n### V\n\n1"), "{rendered}");
    assert!(rendered.contains(markdown::LINK_FOOTER), "{rendered}");
}

#[test]
fn a_paragraph_that_is_a_quoted_string_loses_its_quotes() {
    // Quotes are syntax when they wrap a whole paragraph, and punctuation when
    // they sit inside a sentence.
    let source = r#"export anchor A:
    text:
        In one place we say "The Save Button is Blue" and elsewhere we say
        "The Save Button is Red".

        Example

        "The Save Button is Blue"

        may produce a claim.
"#;
    assert_eq!(
        text_of(&property(source, "A", "text")),
        "In one place we say \"The Save Button is Blue\" and elsewhere we say \"The Save Button is Red\".\nExample\nThe Save Button is Blue\nmay produce a claim."
    );
}

#[test]
fn a_quoted_value_never_coerces_even_when_it_looks_like_another_type() {
    let source = r#"export anchor A:
    looksBoolean:: boolean:: string: "false"
    looksNumeric:: number:: string: "42"
    plainBoolean:: boolean:: string: false
"#;
    assert_eq!(text_of(&property(source, "A", "looksBoolean")), "false");
    assert_eq!(text_of(&property(source, "A", "looksNumeric")), "42");
    assert_eq!(property(source, "A", "plainBoolean"), Value::Bool(false));
}

#[test]
fn an_unconstrained_quoted_value_stays_a_string() {
    let source = "export anchor A:\n    n: \"42\"\n    b: \"true\"\n";
    assert_eq!(text_of(&property(source, "A", "n")), "42");
    assert_eq!(text_of(&property(source, "A", "b")), "true");
}

#[test]
fn a_multi_line_escape_block_is_literal() {
    // Everything the compiler would otherwise read -- a property, a comment, an
    // interpolation -- survives exactly as written.
    let source = "export anchor A:\n    body:\n        \\\\\\\n        key: not a property\n        // not a comment\n        {1 + 2}\n        \\\\\\\n";
    assert_eq!(
        text_of(&property(source, "A", "body")),
        "key: not a property\n// not a comment\n{1 + 2}"
    );
}

#[test]
fn an_escape_block_joins_the_prose_around_it() {
    let source = "export anchor A:\n    body:\n        Before.\n\n        \\\\\\\n        literal: text\n        \\\\\\\n\n        After.\n";
    assert_eq!(
        text_of(&property(source, "A", "body")),
        "Before.\nliteral: text\nAfter."
    );
}

#[test]
fn an_escape_block_inside_a_fence_leaves_only_the_fence() {
    let source = "export anchor A:\n    body:\n        ```piton\n        \\\\\\\n        anchor B:\n            v: ${super.x}\n        \\\\\\\n        ```\n";
    assert_eq!(
        text_of(&property(source, "A", "body")),
        "```piton\nanchor B:\n    v: ${super.x}\n```",
        "the delimiters are consumed and nothing inside is evaluated"
    );
}

#[test]
fn an_escape_block_suppresses_interpolation_without_a_fence() {
    let sandbox = Sandbox::new("escape-no-interp");
    sandbox.file(
        "main.pi",
        "export anchor A:\n    body:\n        \\\\\\\n        ${NotDefined}\n        \\\\\\\n",
    );
    let compilation = sandbox.compile("main.pi");
    assert!(
        !compilation.diagnostics.iter().any(|d| d.is_error()),
        "a literal block resolves no symbols: {:#?}",
        compilation.diagnostics.as_slice()
    );
}
