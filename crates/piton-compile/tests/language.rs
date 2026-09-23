//! The language semantics, checked against the examples in the specification.

use std::path::PathBuf;

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
fn a_user_keyword_is_the_last_base() {
    let source = r#"anchor MyAnchor as my-anchor:
    description: This is a description of my anchor

anchor OtherBase:
    description: Other Base

export my-anchor ChildAnchor extends OtherBase:
    extra: 1
"#;
    // The keyword's anchor is always last in the inheritance chain, so it
    // wins any collision with what's in `extends`.
    assert_eq!(
        text_of(&property(source, "ChildAnchor", "description")),
        "This is a description of my anchor"
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
    // The quotes are part of the value, so it isn't a boolean or a number.
    assert_eq!(
        text_of(&property(source, "A", "quotedFallsThrough")),
        "\"false\""
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
    assert_eq!(
        codes,
        vec!["type-mismatch"],
        "only the bad list should fail"
    );
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
    assert!(
        !found.labels.is_empty(),
        "the declaring anchor is pointed at"
    );
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
fn an_anchor_can_implement_two_abstracts() {
    let sandbox = Sandbox::new("two-abstracts");
    sandbox.file(
        "main.pi",
        "abstract anchor A:\n    a:: string\n\nabstract anchor B:\n    b:: string\n\nexport anchor C extends A, B:\n    a: one\n    b: two\n",
    );
    let compilation = sandbox.compile("main.pi");
    assert!(!compilation.has_errors(), "{:?}", compilation.diagnostics.as_slice());
}

#[test]
fn two_abstracts_with_types_that_do_not_overlap_are_an_error() {
    let sandbox = Sandbox::new("conflicting-abstracts");
    sandbox.file(
        "main.pi",
        "abstract anchor A:\n    x:: string\n\nabstract anchor B:\n    x:: number\n\nexport anchor C extends A, B:\n    x: 1\n",
    );
    let compilation = sandbox.compile("main.pi");
    assert!(compilation
        .diagnostics
        .iter()
        .any(|d| d.code == "conflicting-abstracts"));

    // Overlapping types are fine; the right-most one wins.
    let sandbox = Sandbox::new("overlapping-abstracts");
    sandbox.file(
        "main.pi",
        "abstract anchor A:\n    x:: string\n\nabstract anchor B:\n    x:: simple\n\nexport anchor C extends A, B:\n    x: 1\n",
    );
    assert!(!sandbox.compile("main.pi").has_errors());
}

#[test]
fn a_child_cannot_redeclare_an_inherited_constraint() {
    let sandbox = Sandbox::new("redeclare");
    sandbox.file(
        "main.pi",
        "abstract anchor Card:\n    subtitle:: string:: null\n\nexport anchor A extends Card:\n    subtitle:: string: Hi\n\nexport anchor B extends Card:\n    subtitle: Hi\n",
    );
    let compilation = sandbox.compile("main.pi");
    let redeclared: Vec<_> = compilation
        .diagnostics
        .iter()
        .filter(|d| d.code == "redeclared-constraint")
        .collect();
    assert_eq!(redeclared.len(), 1, "{:?}", compilation.diagnostics.as_slice());
}

#[test]
fn a_constraint_with_no_value_is_only_allowed_in_an_abstract_anchor() {
    let sandbox = Sandbox::new("missing-value");
    sandbox.file(
        "main.pi",
        "export anchor Page:\n    title:: string\n\nexport x:: number\n\nexport anchor Empty:\n    title:\n",
    );
    let compilation = sandbox.compile("main.pi");
    let codes: Vec<&str> = compilation.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes.iter().filter(|c| **c == "missing-value").count(), 2, "{codes:?}");
    assert!(codes.contains(&"empty-value"), "{codes:?}");
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
fn code_blocks_are_parsed_like_any_other_text() {
    // Code blocks are not escaped: a comment inside one is still a comment and
    // a key is still a key.
    let source = "export anchor A:\n    text:\n        before\n\n        ```piton\n        // a comment\n        key: a property\n        ```\n";
    let value = property(source, "A", "text");
    let Value::Mixed(mixed) = value else { panic!("expected an implicit list, got {value:?}") };
    assert_eq!(mixed.get("key"), Some(&Value::string("a property")));
}

#[test]
fn an_escape_block_inside_a_code_block_keeps_it_literal() {
    let source = "export anchor A:\n    text:\n        ```piton\n        \\\\\\\n        // not a comment\n        key: not a property\n        \\\\\\\n        ```\n";
    assert_eq!(
        text_of(&property(source, "A", "text")),
        "```piton\n// not a comment\nkey: not a property\n```"
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
        .file(
            "main.pi",
            "from ./lib import MyAnchor\n\nexport anchor Uses:\n    got: {MyAnchor.v}\n",
        );
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
    sandbox.file("first.pi", "export pi: 3.14\n").file(
        "main.pi",
        "from ./first import pi SliceOf\n\nexport anchor A:\n    v: {SliceOf}\n",
    );
    let compilation = sandbox.compile("main.pi");
    let a = compilation.find_anchor("A").expect("A");
    assert_eq!(
        compilation.properties(a).get("v"),
        Some(&Value::Number(3.14))
    );

    // `pi` itself is not in scope under its original name.
    let sandbox = Sandbox::new("alias-original");
    sandbox.file("first.pi", "export pi: 3.14\n").file(
        "main.pi",
        "from ./first import pi SliceOf\n\nexport anchor A:\n    v: {pi}\n",
    );
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
        .file(
            "first.pi",
            "export anchor MyAnchor:\n    v: 1\nmyVariable: 42\n",
        )
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
        .file(
            "main.pi",
            "use ./keywords\n\nexport anchor A:\n    v: {Custom}\n",
        );
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
    assert!(rendered.contains("[Target](./main.md#target)"), "{rendered}");
    assert!(rendered.contains("## Inline\n\n### V\n\n1"), "{rendered}");
}

#[test]
fn quotes_are_just_characters() {
    // Quotes don't mean anything special, even around a whole paragraph.
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
        "In one place we say \"The Save Button is Blue\" and elsewhere we say \"The Save Button is Red\".\nExample\n\"The Save Button is Blue\"\nmay produce a claim."
    );
}

#[test]
fn a_quoted_value_never_coerces_even_when_it_looks_like_another_type() {
    let source = r#"export anchor A:
    looksBoolean:: boolean:: string: "false"
    looksNumeric:: number:: string: "42"
    plainBoolean:: boolean:: string: false
"#;
    assert_eq!(text_of(&property(source, "A", "looksBoolean")), "\"false\"");
    assert_eq!(text_of(&property(source, "A", "looksNumeric")), "\"42\"");
    assert_eq!(property(source, "A", "plainBoolean"), Value::Bool(false));
}

#[test]
fn an_unconstrained_quoted_value_stays_a_string() {
    let source = "export anchor A:\n    n: \"42\"\n    b: \"true\"\n";
    assert_eq!(text_of(&property(source, "A", "n")), "\"42\"");
    assert_eq!(text_of(&property(source, "A", "b")), "\"true\"");
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

// ---------------------------------------------------------------------------
// Compiling: anything exported from the entry
// ---------------------------------------------------------------------------

/// What the entry compiles to, rendered as JSON.
fn entry_surface(sandbox: &Sandbox, entry: &str) -> String {
    let compilation = sandbox.compile(entry);
    let errors: Vec<String> = compilation
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    let surface = compilation.compiled_surface(compilation.resolution.entry);
    json::declarations(&surface, &compilation)
}

#[test]
fn an_anchor_re_exported_by_the_entry_is_compiled() {
    let sandbox = Sandbox::new("re-export");
    sandbox
        .file("Lonely.pi", "export anchor Lonely:\n    value: 1\n")
        .file(
            "main.pi",
            "from ./Lonely export Lonely\n\nexport anchor Root:\n    value: 2\n",
        );
    let rendered = entry_surface(&sandbox, "main.pi");
    assert!(
        rendered.contains("\"Lonely\""),
        "an export is enough on its own, used or not: {rendered}"
    );
    assert!(rendered.contains("\"Root\""), "{rendered}");
}

#[test]
fn an_index_that_only_forwards_still_compiles_to_something() {
    let sandbox = Sandbox::new("forwarding-index");
    sandbox
        .file("One.pi", "export anchor One:\n    value: 1\n")
        .file("Two.pi", "export anchor Two:\n    value: 2\n")
        .file("main.pi", "from ./One export *\nfrom ./Two export *\n");
    let rendered = entry_surface(&sandbox, "main.pi");
    assert!(rendered.contains("\"One\""), "{rendered}");
    assert!(rendered.contains("\"Two\""), "{rendered}");
}

#[test]
fn a_renamed_export_is_compiled_under_the_name_it_leaves_by() {
    let sandbox = Sandbox::new("renamed-export");
    sandbox
        .file("Thing.pi", "export anchor Thing:\n    value: 1\n")
        .file("main.pi", "from ./Thing export Thing Renamed\n");
    let rendered = entry_surface(&sandbox, "main.pi");
    assert!(rendered.contains("\"Renamed\""), "{rendered}");
    assert!(!rendered.contains("\"Thing\""), "{rendered}");
}

#[test]
fn a_name_the_entry_imports_without_exporting_is_not_a_result_of_its_own() {
    let sandbox = Sandbox::new("import-only");
    sandbox
        .file("Thing.pi", "export anchor Thing:\n    value: 1\n")
        .file(
            "main.pi",
            "from ./Thing import Thing\n\nexport anchor Root:\n    thing: @{Thing}\n",
        );
    let rendered = entry_surface(&sandbox, "main.pi");
    assert!(rendered.contains("\"Root\""), "{rendered}");
    assert!(
        !rendered.contains("\n  \"Thing\""),
        "importing a name borrows it; it does not republish it: {rendered}"
    );
}

#[test]
fn an_abstract_anchor_the_entry_exports_has_no_value_to_compile() {
    let sandbox = Sandbox::new("abstract-export");
    sandbox.file(
        "main.pi",
        "export abstract anchor Shape:\n    value:: number\n\nexport anchor Thing extends Shape:\n    value: 1\n",
    );
    let rendered = entry_surface(&sandbox, "main.pi");
    assert!(rendered.contains("\"Thing\""), "{rendered}");
    assert!(!rendered.contains("\"Shape\""), "{rendered}");
}

// -- rules clarified in the specification ---------------------------------

fn error_codes(source: &str) -> Vec<String> {
    let sandbox = Sandbox::new("codes");
    sandbox.file("main.pi", source);
    let compilation = sandbox.compile("main.pi");
    compilation
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn plus_lines_in_a_string_block_join_with_nothing_and_double_plus_with_a_break() {
    let source = r#"anchor Base:
    d: Base text.

export anchor Plus extends Base:
    d:
        Child text.
        + {super.d}

export anchor DoublePlus extends Base:
    d:
        Child text.
        ++ {super.d}
"#;
    assert_eq!(text_of(&property(source, "Plus", "d")), "Child text.Base text.");
    assert_eq!(text_of(&property(source, "DoublePlus", "d")), "Child text.\nBase text.");
}

#[test]
fn plus_lines_combine_everything_above_them() {
    let source = r#"anchor BaseAnchor:
    items:
        - A
        - B
        - C

export anchor First extends BaseAnchor:
    items:
        + {super.items}
        - D

export anchor Merged extends BaseAnchor:
    items:
        - A
        - D
        + {super.items}
"#;
    assert_eq!(list_of(&property(source, "First", "items")), vec!["A", "B", "C", "D"]);
    assert_eq!(list_of(&property(source, "Merged", "items")), vec!["D", "A", "B", "C"]);
}

#[test]
fn super_finds_a_property_on_an_earlier_base() {
    let source = r#"anchor Left:
    d: from-left

anchor Right:
    other: x

export anchor Child extends Left, Right:
    d: ${super.d} plus child
"#;
    assert_eq!(text_of(&property(source, "Child", "d")), "from-left plus child");
}

#[test]
fn super_keeps_self_on_the_derived_anchor() {
    let source = r#"anchor Base:
    name: Base
    greeting: Hello ${self.name}

export anchor Child extends Base:
    name: Child
    greeting: ${super.greeting}!
"#;
    assert_eq!(text_of(&property(source, "Child", "greeting")), "Hello Child!");
}

#[test]
fn a_reference_can_point_at_a_property() {
    let source = r#"anchor Button:
    color: blue

export anchor A:
    link: @{Button.color}
    same: {@{Button} == @{Button}}
    different: {@{Button} == @{Button.color}}
"#;
    match property(source, "A", "link") {
        Value::Reference(target) => assert_eq!(target.path, vec!["color".to_string()]),
        other => panic!("expected a reference, got {other:?}"),
    }
    assert_eq!(property(source, "A", "same"), Value::Bool(true));
    assert_eq!(property(source, "A", "different"), Value::Bool(false));
}

#[test]
fn a_reference_to_something_that_is_not_an_anchor_is_an_error() {
    let codes = error_codes("x: 1\nexport anchor A:\n    link: @{x}\n");
    assert!(codes.contains(&"invalid-reference".to_string()), "{codes:?}");
}

#[test]
fn a_reference_renders_as_a_path_in_json() {
    let sandbox = Sandbox::new("json-ref");
    sandbox.file("main.pi", "export anchor Button:\n    color: blue\n\nexport link: @{Button.color}\n");
    let compilation = sandbox.compile("main.pi");
    let surface = compilation.compiled_surface(compilation.resolution.entry);
    let rendered = piton_emit::render(
        piton_emit::Adapter::Json,
        &surface,
        &compilation,
        piton_emit::MarkdownContext {
            from_directory: &sandbox.dir,
            source_root: &sandbox.dir,
        },
    );
    assert!(rendered.contains("\"link\": \"./main.json:Button.color\""), "{rendered}");
}

#[test]
fn an_anchor_in_the_middle_of_text_makes_an_implicit_list() {
    let source = "anchor Foo:\n    a: 1\n\nexport anchor A:\n    see: See {Foo} for details\n";
    match property(source, "A", "see") {
        Value::Mixed(mixed) => {
            let items = Value::Mixed(mixed).as_list_items();
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], Value::string("See"));
            assert!(matches!(items[1], Value::Anchor(_)));
            assert_eq!(items[2], Value::string("for details"));
        }
        other => panic!("expected an implicit list, got {other:?}"),
    }
}

#[test]
fn a_string_plus_a_list_is_an_implicit_list_and_the_name_needs_a_string_expression() {
    let source = "tags: [a, b]\n\nexport anchor A:\n    joined: {\"Tags: \" + tags}\n    name: ${tags}\n    flag: {\"Enabled: \" + true}\n";
    assert!(matches!(property(source, "A", "joined"), Value::Mixed(_)));
    assert_eq!(text_of(&property(source, "A", "name")), "tags");
    assert_eq!(text_of(&property(source, "A", "flag")), "Enabled: true");
}

#[test]
fn a_named_property_list_stringifies_to_its_dotted_name() {
    let source = "export anchor A:\n    items: [x, y]\n    name: ${this.items}\n";
    assert_eq!(text_of(&property(source, "A", "name")), "A.items");
}

#[test]
fn a_list_item_is_never_a_key() {
    let source = "export anchor A:\n    h:\n        - Settings:\n            a: 1\n            b: 2\n";
    match property(source, "A", "h") {
        Value::List(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::string("Settings:"));
            assert!(matches!(&items[1], Value::Dict(map) if map.len() == 2));
        }
        other => panic!("expected a list, got {other:?}"),
    }
}

#[test]
fn a_literal_coerced_to_a_string_keeps_its_text() {
    let source = "export anchor A:\n    written:: string: 1.0\n    evaluated:: string: {1.0}\n";
    assert_eq!(text_of(&property(source, "A", "written")), "1.0");
    assert_eq!(text_of(&property(source, "A", "evaluated")), "1");
}

#[test]
fn booleans_and_null_never_coerce_to_strings() {
    assert!(error_codes("export flag:: string: false\n").contains(&"type-mismatch".to_string()));
    assert!(error_codes("export flag:: string: null\n").contains(&"type-mismatch".to_string()));
}

#[test]
fn logical_operators_and_the_ternary_need_booleans() {
    assert!(error_codes("export a: {1 ? 2 : 3}\n").contains(&"invalid-condition".to_string()));
    assert!(error_codes("export a: {1 && true}\n").contains(&"invalid-operand".to_string()));
    assert!(error_codes("export a: {!1}\n").contains(&"invalid-operand".to_string()));
}

#[test]
fn plus_on_values_it_cannot_combine_is_an_error() {
    assert!(error_codes("export a: {true + 1}\n").contains(&"invalid-operand".to_string()));
}

#[test]
fn double_plus_merges_dictionaries_deeply() {
    let source = "x:\n    a:\n        p: 1\ny:\n    a:\n        q: 2\n\nexport anchor A:\n    shallow: {x + y}\n    deep: {x ++ y}\n";
    let deep = property(source, "A", "deep");
    let Value::Dict(deep) = deep else { panic!("expected a dictionary") };
    assert!(matches!(deep.get("a"), Some(Value::Dict(a)) if a.len() == 2));
    let Value::Dict(shallow) = property(source, "A", "shallow") else { panic!() };
    assert!(matches!(shallow.get("a"), Some(Value::Dict(a)) if a.len() == 1));
}

#[test]
fn a_hyphen_inside_a_name_is_part_of_the_name() {
    let source = "export anchor K:\n    foo-bar: w\n    read: {this.foo-bar}\n";
    assert_eq!(text_of(&property(source, "K", "read")), "w");
}

#[test]
fn a_decimal_needs_its_leading_zero() {
    let source = "export anchor N:\n    a: .5\n    b: 0.5\n    c: 1e3\n";
    assert_eq!(property(source, "N", "a"), Value::string(".5"));
    assert_eq!(property(source, "N", "b"), Value::Number(0.5));
    assert_eq!(property(source, "N", "c"), Value::string("1e3"));
}

#[test]
fn an_escaped_comment_marker_is_text() {
    let source = "export anchor A:\n    b: \\ // \\ Just Text\n";
    assert_eq!(text_of(&property(source, "A", "b")), "// Just Text");
}

#[test]
fn compiled_output_holds_only_exports() {
    let sandbox = Sandbox::new("exports-only");
    sandbox.file(
        "main.pi",
        "anchor Hidden:\n    a: 1\n\nabstract anchor Shape:\n    a:: number\n\nexport anchor Shown:\n    b: 2\n\nx: 3\n",
    );
    let compilation = sandbox.compile("main.pi");
    let surface = compilation.compiled_surface(compilation.resolution.entry);
    let keys: Vec<&str> = surface.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["Shown"]);
}

#[test]
fn syntax_problems_are_reported_by_the_compilation() {
    let codes = error_codes("garbage line here\n");
    assert!(codes.contains(&"invalid-declaration".to_string()), "{codes:?}");
    let codes = error_codes("export anchor A:\n    a: 1\n\tb: 2\n");
    assert!(codes.contains(&"inconsistent-indentation".to_string()), "{codes:?}");
}

#[test]
fn syntax_rules_from_the_overview_are_enforced() {
    assert!(error_codes("anchor A as list:\n    a: 1\n").contains(&"reserved-keyword".to_string()));
    assert!(error_codes("export anchor A:\nexport x: 1\n").contains(&"empty-anchor".to_string()));
    assert!(error_codes("export myVariable::number:42\n").contains(&"constraint-spacing".to_string()));
    assert!(error_codes("export anchor A:\n    x::number: 1\n").contains(&"constraint-spacing".to_string()));
    assert!(error_codes("export anchor A:\n    a.b: 1\n").contains(&"invalid-key".to_string()));
}

#[test]
fn an_inheritance_cycle_shows_the_cycle() {
    let sandbox = Sandbox::new("cycle");
    sandbox.file("main.pi", "anchor A extends B:\n    x: 1\n\nanchor B extends A:\n    y: 1\n");
    let compilation = sandbox.compile("main.pi");
    assert!(compilation
        .diagnostics
        .iter()
        .any(|d| d.code == "inheritance-cycle" && d.message.contains("A -> B -> A")));
}

#[test]
fn a_plain_anchor_type_takes_the_winning_abstract() {
    let sandbox = Sandbox::new("winning-abstract");
    sandbox.file(
        "main.pi",
        "abstract anchor A:\n    a:: string\n\nabstract anchor B:\n    b:: string\n\nanchor Both extends A, B:\n    a: one\n    b: two\n\nexport anchor Holder:\n    onlyB:: B[]: [{Both}]\n",
    );
    assert!(!sandbox.compile("main.pi").has_errors());
    sandbox.file(
        "main.pi",
        "abstract anchor A:\n    a:: string\n\nabstract anchor B:\n    b:: string\n\nanchor Both extends A, B:\n    a: one\n    b: two\n\nexport anchor Holder:\n    onlyA:: A[]: [{Both}]\n",
    );
    assert!(sandbox.compile("main.pi").has_errors());
}

#[test]
fn a_duplicate_key_in_one_block_is_an_error() {
    assert!(error_codes("export anchor A:\n    a: 1\n    a: 2\n").contains(&"duplicate-key".to_string()));
}

#[test]
fn wrapping_is_the_only_way_to_escape() {
    let source = "export anchor A:\n    wrapped: Similarly\\ : \\\n    bare: back\\slash\n";
    assert_eq!(text_of(&property(source, "A", "wrapped")), "Similarly:");
    assert_eq!(text_of(&property(source, "A", "bare")), "back\\slash");
}
