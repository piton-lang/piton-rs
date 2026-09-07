//! End-to-end checks of the examples in the Piton specification.

use piton_core::compile::compile;
use piton_core::db::Db;
use piton_core::framework::Frameworks;
use piton_core::serialize::to_json;
use piton_core::{builtin, FileId};

/// Compile one source string and return `name` as JSON.
fn eval(source: &str, name: &str) -> serde_json::Value {
    let (json, diagnostics) = eval_with_diagnostics(source, name);
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
    json
}

fn eval_with_diagnostics(source: &str, name: &str) -> (serde_json::Value, Vec<String>) {
    let mut db = Db::new();
    for module in builtin::modules() {
        db.add_virtual_module(module.name, module.source);
    }
    db.add_virtual_module("@test/main", source);
    let entry = db.resolve(FileId(0), "@test/main").expect("virtual module loads");
    let compilation = compile(db, vec![entry], &Frameworks::default());
    let messages: Vec<String> =
        compilation.diagnostics.iter().map(|it| it.message.clone()).collect();
    let value = compilation
        .file_values(entry)
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| to_json(&value))
        .unwrap_or(serde_json::Value::Null);
    (value, messages)
}

macro_rules! json {
    ($($tokens:tt)*) => { serde_json::json!($($tokens)*) };
}

#[test]
fn scalars_and_inference() {
    assert_eq!(eval("a: 42\n", "a"), json!(42.0));
    assert_eq!(eval("a: 42 things\n", "a"), json!("42 things"));
    assert_eq!(eval("a: true\n", "a"), json!(true));
    assert_eq!(eval("a: true story\n", "a"), json!("true story"));
    assert_eq!(eval("a: null\n", "a"), json!(null));
    assert_eq!(eval("a: 2.71\n", "a"), json!(2.71));
    assert_eq!(eval("a: 0.14\n", "a"), json!(0.14));
    assert_eq!(eval("a: 1_200_000.00\n", "a"), json!(1200000.0));
    assert_eq!(eval("a: This costs $5 + tax\n", "a"), json!("This costs $5 + tax"));
    assert_eq!(eval("a: \\// Just Text\n", "a"), json!("// Just Text"));
    assert_eq!(eval("a: \"// Also Just Text\"\n", "a"), json!("// Also Just Text"));
    assert_eq!(eval("a: \"false\"\n", "a"), json!("false"));
    assert_eq!(eval("a: 42 // trailing comment\n", "a"), json!(42.0));
}

#[test]
fn strings_span_lines_and_blank_lines_break() {
    let source = "a:\n    This broken string is not considered\n    a line break.\n\n    While this *is* on a new line because there was a blank line above.\n";
    assert_eq!(
        eval(source, "a"),
        json!("This broken string is not considered a line break.\nWhile this *is* on a new line because there was a blank line above.")
    );
}

#[test]
fn lists_in_both_styles() {
    assert_eq!(eval("a:\n    - One\n    - Two\n", "a"), json!(["One", "Two"]));
    assert_eq!(eval("a: [This, is, a, list]\n", "a"), json!(["This", "is", "a", "list"]));
    assert_eq!(
        eval("a:\n    - Level 1\n        - Level 2\n            - Level 3\n", "a"),
        json!(["Level 1", ["Level 2", ["Level 3"]]])
    );
    assert_eq!(
        eval("a: [Level 1, [Level 2, [Level 3]]]\n", "a"),
        json!(["Level 1", ["Level 2", ["Level 3"]]])
    );
}

#[test]
fn dictionaries_nest_and_are_addressable() {
    let source = "a:\n    b:\n        c: This is a string\nd: {a.b.c}\n";
    assert_eq!(eval(source, "d"), json!("This is a string"));
}

#[test]
fn mixed_blocks_become_implicit_lists() {
    let source = "\
combined:
    This is a string

    - This
    - Is
    - A
    - List

    nestedDictionary:
        deeplyNestedDictionary: This is a string

reached: {combined.nestedDictionary.deeplyNestedDictionary}
";
    assert_eq!(
        eval(source, "combined"),
        json!([
            "This is a string",
            ["This", "Is", "A", "List"],
            { "nestedDictionary": { "deeplyNestedDictionary": "This is a string" } }
        ])
    );
    assert_eq!(eval(source, "reached"), json!("This is a string"));
}

#[test]
fn expressions_only_evaluate_with_literal_atoms() {
    assert_eq!(eval("a: 1 + 2\n", "a"), json!(3.0));
    assert_eq!(eval("a: 2.71 - 2.71\n", "a"), json!(0.0));
    assert_eq!(eval("a: 2 + Hello\n", "a"), json!("2 + Hello"));
    assert_eq!(eval("a: 1\nb: 2\nresult: a + b\n", "result"), json!("a + b"));
    assert_eq!(eval("a: 1\nb: 2\nresult: {a + b}\n", "result"), json!(3.0));
    assert_eq!(eval("a: \"This is a\" + \" string\"\n", "a"), json!("This is a string"));
    assert_eq!(eval("a: \"The number is \" + 5\n", "a"), json!("The number is 5"));
    assert_eq!(eval("a: \"Hello, World\" + [1, 2, 3]\n", "a"), json!(["Hello, World", [1.0, 2.0, 3.0]]));
}

#[test]
fn concatenation_deduplicates_with_plus_only() {
    assert_eq!(eval("a: [1, 2, 3] + [1, 2, 3]\n", "a"), json!([1.0, 2.0, 3.0]));
    assert_eq!(eval("a: [1, 2, 3, 4] + [1, 2, 3]\n", "a"), json!([4.0, 1.0, 2.0, 3.0]));
    assert_eq!(
        eval("a: [1, 2, 3] ++ [1, 2, 3]\n", "a"),
        json!([1.0, 2.0, 3.0, 1.0, 2.0, 3.0])
    );
}

#[test]
fn operators_follow_precedence() {
    assert_eq!(eval("a: {1 + 2 * 3}\n", "a"), json!(7.0));
    assert_eq!(eval("a: {(1 + 2) * 3}\n", "a"), json!(9.0));
    assert_eq!(eval("a: {7 % 3}\n", "a"), json!(1.0));
    assert_eq!(eval("a: {-7 % 3}\n", "a"), json!(2.0));
    assert_eq!(eval("a: {true && false}\n", "a"), json!(false));
    assert_eq!(eval("a: {false || true}\n", "a"), json!(true));
    assert_eq!(eval("a: {1 == 1 ? \"yes\" : \"no\"}\n", "a"), json!("yes"));
    assert_eq!(eval("a: {null == null}\n", "a"), json!(true));
}

#[test]
fn type_constraints_pick_the_first_that_fits() {
    assert_eq!(eval("a:: number: 42\n", "a"), json!(42.0));
    assert_eq!(eval("a:: number:: string: 42\n", "a"), json!(42.0));
    assert_eq!(eval("a:: string:: number: 42\n", "a"), json!("42"));
    assert_eq!(eval("a:: boolean:: number:: string: \"false\"\n", "a"), json!("false"));
    assert_eq!(eval("a:: string[]: [foo, bar]\n", "a"), json!(["foo", "bar"]));
    assert_eq!(eval("a:: simple: 1\n", "a"), json!(1.0));
    assert_eq!(eval("a:: complex: [1, 2, 3]\n", "a"), json!([1.0, 2.0, 3.0]));
    assert_eq!(eval("a:: any: Hello, World\n", "a"), json!("Hello, World"));

    let (_, diagnostics) = eval_with_diagnostics("a:: number: not a number\n", "a");
    assert!(diagnostics.iter().any(|it| it.contains("does not satisfy")), "{diagnostics:?}");
}

#[test]
fn interpolation_reads_simple_values() {
    assert_eq!(eval("name: Piton\nmessage: Hello, World from ${name}\n", "message"), json!("Hello, World from Piton"));
    assert_eq!(eval("version: 1.0\nmessage: Piton is at version ${version}\n", "message"), json!("Piton is at version 1.0"));
}

#[test]
fn anchors_compile_to_dictionaries() {
    let source = "\
anchor MyFirstAnchor:
    whatIsAnAnchor:
        An anchor is a kind of object or document that is structured via properties
        and values.

    stringValue: String types are supported.

    listTypes:
        - List types
        - are
        - supported.

    numberTypes: 2.71 - 2.71

    booleanTypes: true

    nullType: null

    booleanOperatorsAnd: {this.booleanTypes} && false
    booleanOperatorsOr: {this.booleanTypes} || false
";
    assert_eq!(
        eval(source, "MyFirstAnchor"),
        json!({
            "whatIsAnAnchor": "An anchor is a kind of object or document that is structured via properties and values.",
            "stringValue": "String types are supported.",
            "listTypes": ["List types", "are", "supported."],
            "numberTypes": 0.0,
            "booleanTypes": true,
            "nullType": null,
            "booleanOperatorsAnd": false,
            "booleanOperatorsOr": true
        })
    );
}

#[test]
fn inheritance_resolves_left_to_right() {
    let source = "\
anchor FirstBaseAnchor:
    firstBaseAnchorProperty: Hello from the First Base Anchor
    description: Description from FirstBaseAnchor

anchor SecondBaseAnchor:
    secondBaseAnchorProperty: Hello from the Second Base Anchor
    description: Description from SecondBaseAnchor

anchor ChildAnchor extends FirstBaseAnchor, SecondBaseAnchor:
    childAnchorProperty: Hello from Child Anchor
";
    assert_eq!(
        eval(source, "ChildAnchor"),
        json!({
            "firstBaseAnchorProperty": "Hello from the First Base Anchor",
            "description": "Description from SecondBaseAnchor",
            "secondBaseAnchorProperty": "Hello from the Second Base Anchor",
            "childAnchorProperty": "Hello from Child Anchor"
        })
    );
}

#[test]
fn super_reads_the_base_definition() {
    let source = "\
anchor BaseAnchor:
    description: Description from BaseAnchor

anchor ChildAnchor extends BaseAnchor:
    description:
        ${super.description} and Description from ChildAnchor
";
    assert_eq!(
        eval(source, "ChildAnchor"),
        json!({ "description": "Description from BaseAnchor and Description from ChildAnchor" })
    );
}

#[test]
fn super_spreads_into_lists() {
    let base = "anchor BaseAnchor:\n    items:\n        - A\n        - B\n        - C\n\n";
    assert_eq!(
        eval(&format!("{base}anchor ChildAnchor extends BaseAnchor:\n    items:\n        + {{super.items}}\n        - D\n        - E\n        - F\n"), "ChildAnchor"),
        json!({ "items": ["A", "B", "C", "D", "E", "F"] })
    );
    assert_eq!(
        eval(&format!("{base}anchor ChildAnchor extends BaseAnchor:\n    items:\n        - A\n        - B\n        - C\n        - D\n        + {{super.items}}\n"), "ChildAnchor"),
        json!({ "items": ["D", "A", "B", "C"] })
    );
    assert_eq!(
        eval(&format!("{base}anchor ChildAnchor extends BaseAnchor:\n    items:\n        - A\n        - B\n        - C\n        - D\n        ++ {{super.items}}\n"), "ChildAnchor"),
        json!({ "items": ["A", "B", "C", "D", "A", "B", "C"] })
    );
}

#[test]
fn self_travels_and_this_is_pinned() {
    let source = "\
anchor Base:
    name: Base Anchor
    baseDescription: This is ${this.name}

anchor MidChild extends Base:
    name: MidChild Anchor
    baseDescription: Override on baseDescription ${this.name}
    childDescription: This is ${self.name}

anchor FinalChild extends MidChild:
    name: FinalChild Anchor
";
    assert_eq!(
        eval(source, "FinalChild"),
        json!({
            "name": "FinalChild Anchor",
            "baseDescription": "Override on baseDescription MidChild Anchor",
            "childDescription": "This is FinalChild Anchor"
        })
    );

    let simple = "\
anchor Base:
    name: Base Anchor
    baseDescription: This is ${self.name}

anchor Child extends Base:
    name: Child Anchor
    childDescription: This is ${self.name}
";
    assert_eq!(
        eval(simple, "Child"),
        json!({
            "name": "Child Anchor",
            "baseDescription": "This is Child Anchor",
            "childDescription": "This is Child Anchor"
        })
    );
}

#[test]
fn user_defined_keywords_are_sugar_for_extends() {
    let source = "\
anchor MyAnchor as my-anchor:
    description: This is a description of my anchor

my-anchor ChildAnchor:
    description:
        + {super.description}
        My additional description
";
    let value = eval(source, "ChildAnchor");
    assert_eq!(
        value,
        json!({ "description": ["This is a description of my anchor", "My additional description"] })
    );
}

#[test]
fn abstracts_must_be_implemented() {
    let good = "\
abstract anchor Skill:
    description:: string

anchor ConcreteSkill extends Skill:
    description: This must be a string as defined by the abstract
";
    assert_eq!(
        eval(good, "ConcreteSkill"),
        json!({ "description": "This must be a string as defined by the abstract" })
    );

    let bad = "\
abstract anchor Skill:
    description:: string

anchor Broken extends Skill:
    other: value
";
    let (_, diagnostics) = eval_with_diagnostics(bad, "Broken");
    assert!(diagnostics.iter().any(|it| it.contains("must define `description`")), "{diagnostics:?}");
}

#[test]
fn references_and_cycles() {
    let source = "myList: [1, 2, 3]\n\nmyDictionary:\n    list: {myList}\n\nnewList: {myDictionary.list + [4, 5, 6]}\n";
    assert_eq!(eval(source, "newList"), json!([1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));

    let (_, diagnostics) = eval_with_diagnostics("a: {b}\nb: {a}\n", "a");
    assert!(diagnostics.iter().any(|it| it.contains("refers to itself")), "{diagnostics:?}");

    let (_, diagnostics) = eval_with_diagnostics("a: {missing}\n", "a");
    assert!(diagnostics.iter().any(|it| it.contains("cannot find")), "{diagnostics:?}");
}

#[test]
fn division_by_zero_is_an_error() {
    let (_, diagnostics) = eval_with_diagnostics("a: {1 / 0}\n", "a");
    assert!(diagnostics.iter().any(|it| it.contains("division by zero")), "{diagnostics:?}");
}

#[test]
fn mismatched_comparison_is_an_error() {
    let (_, diagnostics) = eval_with_diagnostics("a: {1 == yes}\n", "a");
    assert!(diagnostics.iter().any(|it| it.contains("cannot compare")), "{diagnostics:?}");
}

#[test]
fn conflicting_abstract_constraints_are_an_error() {
    let source = "\
abstract anchor A:
    value:: string

abstract anchor B:
    value:: number

abstract anchor C extends A, B:
    other:: string
";
    let (_, diagnostics) = eval_with_diagnostics(source, "C");
    assert!(
        diagnostics.iter().any(|it| it.contains("is constrained to")),
        "{diagnostics:?}"
    );
}

#[test]
fn extends_constraints_are_abstract_only() {
    let source = "\
abstract anchor A:
    description:: string

abstract anchor Holder:
    items:: extends A[]

anchor Bad:
    items:: extends A[]
";
    let (_, diagnostics) = eval_with_diagnostics(source, "Bad");
    assert!(
        diagnostics.iter().any(|it| it.contains("only allowed inside abstract anchors")),
        "{diagnostics:?}"
    );
}

#[test]
fn anchor_type_constraints_check_the_inheritance_chain() {
    let source = "\
abstract anchor A:
    description:: string

abstract anchor B extends A:
    name:: string

anchor ImplementsB extends B:
    description: from b
    name: b

abstract anchor Holder:
    direct:: A
    chain:: extends A

anchor Uses extends Holder:
    direct: {ImplementsB}
    chain: {ImplementsB}
";
    let (_, diagnostics) = eval_with_diagnostics(source, "Uses");
    // `A` alone wants a direct implementer; `extends A` accepts the whole chain.
    assert!(
        diagnostics.iter().any(|it| it.contains("anchor does not satisfy A")),
        "{diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|it| it.contains("does not satisfy extends A")),
        "{diagnostics:?}"
    );
}
