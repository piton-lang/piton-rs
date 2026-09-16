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
    assert_eq!(eval("a: 42\n", "a"), json!(42));
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
    assert_eq!(eval("a: 42 // trailing comment\n", "a"), json!(42));
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
    assert_eq!(eval("a: 1 + 2\n", "a"), json!(3));
    assert_eq!(eval("a: 2.71 - 2.71\n", "a"), json!(0));
    assert_eq!(eval("a: 2 + Hello\n", "a"), json!("2 + Hello"));
    assert_eq!(eval("a: 1\nb: 2\nresult: a + b\n", "result"), json!("a + b"));
    assert_eq!(eval("a: 1\nb: 2\nresult: {a + b}\n", "result"), json!(3));
    assert_eq!(eval("a: \"This is a\" + \" string\"\n", "a"), json!("This is a string"));
    assert_eq!(eval("a: \"The number is \" + 5\n", "a"), json!("The number is 5"));
    assert_eq!(eval("a: \"Hello, World\" + [1, 2, 3]\n", "a"), json!(["Hello, World", [1, 2, 3]]));
}

#[test]
fn concatenation_deduplicates_with_plus_only() {
    assert_eq!(eval("a: [1, 2, 3] + [1, 2, 3]\n", "a"), json!([1, 2, 3]));
    assert_eq!(eval("a: [1, 2, 3, 4] + [1, 2, 3]\n", "a"), json!([4, 1, 2, 3]));
    assert_eq!(
        eval("a: [1, 2, 3] ++ [1, 2, 3]\n", "a"),
        json!([1, 2, 3, 1, 2, 3])
    );
}

#[test]
fn operators_follow_precedence() {
    assert_eq!(eval("a: {1 + 2 * 3}\n", "a"), json!(7));
    assert_eq!(eval("a: {(1 + 2) * 3}\n", "a"), json!(9));
    assert_eq!(eval("a: {7 % 3}\n", "a"), json!(1));
    assert_eq!(eval("a: {-7 % 3}\n", "a"), json!(2));
    assert_eq!(eval("a: {true && false}\n", "a"), json!(false));
    assert_eq!(eval("a: {false || true}\n", "a"), json!(true));
    assert_eq!(eval("a: {1 == 1 ? \"yes\" : \"no\"}\n", "a"), json!("yes"));
    assert_eq!(eval("a: {null == null}\n", "a"), json!(true));
}

#[test]
fn type_constraints_pick_the_first_that_fits() {
    assert_eq!(eval("a:: number: 42\n", "a"), json!(42));
    assert_eq!(eval("a:: number:: string: 42\n", "a"), json!(42));
    assert_eq!(eval("a:: string:: number: 42\n", "a"), json!("42"));
    assert_eq!(eval("a:: boolean:: number:: string: \"false\"\n", "a"), json!("false"));
    assert_eq!(eval("a:: string[]: [foo, bar]\n", "a"), json!(["foo", "bar"]));
    assert_eq!(eval("a:: simple: 1\n", "a"), json!(1));
    assert_eq!(eval("a:: complex: [1, 2, 3]\n", "a"), json!([1, 2, 3]));
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
            "numberTypes": 0,
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
    assert_eq!(eval(source, "newList"), json!([1, 2, 3, 4, 5, 6]));

    let (_, diagnostics) = eval_with_diagnostics("a: {b}\nb: {a}\n", "a");
    assert!(diagnostics.iter().any(|it| it.contains("refers to itself")), "{diagnostics:?}");

    let (_, diagnostics) = eval_with_diagnostics("a: {missing}\n", "a");
    assert!(diagnostics.iter().any(|it| it.contains("cannot find")), "{diagnostics:?}");
}

#[test]
fn anchors_may_refer_to_each_other_in_prose() {
    // A circular reference is only an error when it cannot be resolved. Two
    // anchors that mention each other settle to a string each, so they compile.
    let prose = "\
anchor A:
    description: This anchor talks about ${B}

anchor B:
    description: This anchor talks about ${A}
";
    assert_eq!(eval(prose, "A"), json!({ "description": "This anchor talks about B" }));
    assert_eq!(eval(prose, "B"), json!({ "description": "This anchor talks about A" }));

    // An anchor naming itself settles the same way.
    assert_eq!(
        eval("anchor A:\n    name: A Anchor\n    mention: This is ${self}\n", "A"),
        json!({ "name": "A Anchor", "mention": "This is A" })
    );

    // Reading one property of each other resolves too, because properties are
    // resolved one at a time.
    assert_eq!(
        eval("anchor A:\n    x: 1\n    y: {B.z}\n\nanchor B:\n    z: {A.x}\n", "A"),
        json!({ "x": 1, "y": 1 })
    );

    // A property that depends on itself, the long way round, still cannot.
    let (_, diagnostics) =
        eval_with_diagnostics("anchor A:\n    x: {B.z}\n\nanchor B:\n    z: {A.x}\n", "A");
    assert!(diagnostics.iter().any(|it| it.contains("refers to itself")), "{diagnostics:?}");

    // Nor can a value that *is* the other anchor: that structure has no end.
    let (_, diagnostics) =
        eval_with_diagnostics("anchor A:\n    b: {B}\n\nanchor B:\n    a: {A}\n", "A");
    assert!(diagnostics.iter().any(|it| it.contains("refers to itself")), "{diagnostics:?}");
}

#[test]
fn division_by_zero_is_an_error() {
    let (_, diagnostics) = eval_with_diagnostics("a: {1 / 0}\n", "a");
    assert!(diagnostics.iter().any(|it| it.contains("division by zero")), "{diagnostics:?}");
}

#[test]
fn mismatched_comparison_is_an_error() {
    // Quoted, because a bare `yes` is a name the compiler cannot find, and a
    // comparison with something that already failed is not reported again.
    let (_, diagnostics) = eval_with_diagnostics("a: {1 == \"yes\"}\n", "a");
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
fn an_extends_list_constraint_accepts_the_whole_chain() {
    // `extends A[]` reads as a list of things whose chain includes `A`. The
    // `[]` binds to the name, so the `extends` has to be pushed down onto the
    // element type or the constraint can never be satisfied at all.
    let source = "\
abstract anchor A:
    description:: string

abstract anchor B extends A:
    name:: string

anchor ImplementsA extends A:
    description: a

anchor ImplementsB extends B:
    description: b
    name: b

anchor Unrelated:
    description: neither

abstract anchor Holder:
    chain:: extends A[]

anchor Uses extends Holder:
    chain: [{ImplementsA}, {ImplementsB}]
";
    let (_, diagnostics) = eval_with_diagnostics(source, "Uses");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let rejected = source.replace("[{ImplementsA}, {ImplementsB}]", "[{Unrelated}]");
    let (_, diagnostics) = eval_with_diagnostics(&rejected, "Uses");
    assert!(
        diagnostics.iter().any(|it| it.contains("does not satisfy extends A[]")),
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

// ---- the remaining rows of the specification's tables -----------------------

#[test]
fn escapes_and_quoting_produce_strings() {
    assert_eq!(eval("a: \\false\n", "a"), json!("false"));
    assert_eq!(eval("a: \\// not a comment\n", "a"), json!("// not a comment"));
    assert_eq!(eval("a: \"true\"\n", "a"), json!("true"));
    assert_eq!(eval("a: he said \"hi\" loudly\n", "a"), json!("he said \"hi\" loudly"));
    assert_eq!(eval("a: \"a \\\" quote\"\n", "a"), json!("a \" quote"));
}

#[test]
fn truthiness_follows_the_specification() {
    for (source, expected) in [
        ("a: {0 ? \"t\" : \"f\"}\n", "f"),
        ("a: {1 ? \"t\" : \"f\"}\n", "t"),
        ("a: {-1 ? \"t\" : \"f\"}\n", "t"),
        ("a: {null ? \"t\" : \"f\"}\n", "f"),
        ("a: {false ? \"t\" : \"f\"}\n", "f"),
        ("a: {\"\" ? \"t\" : \"f\"}\n", "f"),
        ("a: {\"x\" ? \"t\" : \"f\"}\n", "t"),
    ] {
        assert_eq!(eval(source, "a"), json!(expected), "{source}");
    }
}

#[test]
fn logical_operators_short_circuit() {
    // The right-hand side would divide by zero if it were evaluated.
    let (_, diagnostics) = eval_with_diagnostics("a: {false && (1 / 0)}\n", "a");
    assert!(diagnostics.is_empty(), "`&&` must not evaluate its right side: {diagnostics:?}");
    let (_, diagnostics) = eval_with_diagnostics("a: {true || (1 / 0)}\n", "a");
    assert!(diagnostics.is_empty(), "`||` must not evaluate its right side: {diagnostics:?}");
    assert_eq!(eval("a: {false && true}\n", "a"), json!(false));
    assert_eq!(eval("a: {true || false}\n", "a"), json!(true));
}

#[test]
fn modulo_takes_the_divisors_sign() {
    assert_eq!(eval("a: {7 % 3}\n", "a"), json!(1));
    assert_eq!(eval("a: {-7 % 3}\n", "a"), json!(2));
    assert_eq!(eval("a: {7 % -3}\n", "a"), json!(-2));
    assert_eq!(eval("a: {6 % 3}\n", "a"), json!(0));
}

#[test]
fn the_ternary_is_right_associative_and_loosest() {
    // Parsed as `true ? "a" : (false ? "b" : "c")`.
    assert_eq!(eval("a: {true ? \"a\" : false ? \"b\" : \"c\"}\n", "a"), json!("a"));
    assert_eq!(eval("a: {false ? \"a\" : false ? \"b\" : \"c\"}\n", "a"), json!("c"));
    // `||` binds tighter than the ternary.
    assert_eq!(eval("a: {false || true ? \"y\" : \"n\"}\n", "a"), json!("y"));
}

#[test]
fn dictionaries_merge_shallowly_with_plus_and_deeply_with_plus_plus() {
    let source = "\
left:
    keep: 1
    nested:
        a: 1
        b: 2

right:
    nested:
        b: 20
        c: 30

shallow: {left + right}
deep: {left ++ right}
";
    assert_eq!(
        eval(source, "shallow"),
        json!({ "keep": 1, "nested": { "b": 20, "c": 30 } }),
        "`+` replaces the nested dictionary"
    );
    assert_eq!(
        eval(source, "deep"),
        json!({ "keep": 1, "nested": { "a": 1, "b": 20, "c": 30 } }),
        "`++` merges into it"
    );
}

#[test]
fn a_dictionary_may_be_written_as_a_list_element() {
    // "I'm just pointing this out because it is syntactically possible."
    // The key is a key, not the text `dictionaryInsideAList:`, and its block
    // belongs to it rather than becoming a sibling element.
    let source = "\
combined:
  - dictionaryInsideAList:
      nested: value
";
    assert_eq!(
        eval(source, "combined"),
        json!([{ "dictionaryInsideAList": { "nested": "value" } }])
    );
    assert_eq!(
        eval("a:\n    - key: value\n    - other: thing\n", "a"),
        json!([{ "key": "value" }, { "other": "thing" }])
    );
    // And the dictionary is not addressable, because the list is explicit.
    let (_, diagnostics) =
        eval_with_diagnostics("combined:\n    - key: value\nreach: {combined.key}\n", "reach");
    assert!(
        diagnostics.iter().any(|it| it.contains("has no property `key`")),
        "{diagnostics:?}"
    );
}

#[test]
fn whole_numbers_serialize_as_integers() {
    // Piton has one number type, but the compiled JSON should read the way the
    // specification writes it: `0`, not `0.0`.
    assert_eq!(eval("a: 1 + 2\n", "a").to_string(), "3");
    assert_eq!(eval("a: 3.14 - 3.14\n", "a").to_string(), "0");
    // A number the author spelled with a fraction keeps it.
    assert_eq!(eval("a: 1.0\n", "a").to_string(), "1.0");
    assert_eq!(eval("a: 1_200_000.00\n", "a").to_string(), "1200000.0");
}

#[test]
fn list_deduplication_is_shallow() {
    // Equal nested lists are the same value, so they deduplicate.
    assert_eq!(eval("a: {[[1, 2]] + [[1, 2]]}\n", "a"), json!([[1, 2]]));
    // Different ones do not.
    assert_eq!(eval("a: {[[1, 2]] + [[3]]}\n", "a"), json!([[1, 2], [3]]));
}

#[test]
fn only_dictionaries_in_an_implicit_list_stay_addressable() {
    let addressable = "\
combined:
    prose

    key:
        deep: found

reached: {combined.key.deep}
";
    assert_eq!(eval(addressable, "reached"), json!("found"));

    // Inside an explicit list, the dictionary is out of reach.
    let hidden = "combined:\n    - key:\n        deep: found\n\nreached: {combined.key}\n";
    let (_, diagnostics) = eval_with_diagnostics(hidden, "reached");
    assert!(diagnostics.iter().any(|it| it.contains("has no property")), "{diagnostics:?}");
}

#[test]
fn special_constraints_accept_and_reject_the_right_shapes() {
    assert_eq!(eval("a:: simple: false\n", "a"), json!(false));
    assert_eq!(eval("a:: any:\n    - 1\n", "a"), json!([1]));
    assert_eq!(eval("a:: null: null\n", "a"), json!(null));

    for (source, name) in [
        ("a:: simple:\n    - one\n", "simple rejects a list"),
        ("a:: complex: 1\n", "complex rejects a number"),
        ("a:: number: true\n", "number rejects a boolean"),
        ("a:: boolean: 1\n", "boolean rejects a number"),
        ("a:: string:\n    - one\n", "string rejects a list"),
        ("a:: dictionary:\n    - one\n", "dictionary rejects a list"),
    ] {
        let (_, diagnostics) = eval_with_diagnostics(source, "a");
        assert!(
            diagnostics.iter().any(|it| it.contains("does not satisfy")),
            "{name}: {diagnostics:?}"
        );
    }
}

#[test]
fn comparisons_work_within_a_type_and_not_across_them() {
    assert_eq!(eval("a: {1 < 2}\n", "a"), json!(true));
    assert_eq!(eval("a: {\"a\" < \"b\"}\n", "a"), json!(true));
    assert_eq!(eval("a: {\"x\" == \"x\"}\n", "a"), json!(true));
    assert_eq!(eval("a: {null != null}\n", "a"), json!(false));
    let (_, diagnostics) = eval_with_diagnostics("a: {1 < \"x\"}\n", "a");
    assert!(diagnostics.iter().any(|it| it.contains("cannot order")), "{diagnostics:?}");
}

#[test]
fn a_user_defined_keyword_must_be_lowercase_and_unreserved() {
    let (_, diagnostics) = eval_with_diagnostics("anchor A as MyKeyword:\n    x: 1\n", "A");
    assert!(diagnostics.iter().any(|it| it.contains("must be lowercase")), "{diagnostics:?}");

    let (_, diagnostics) = eval_with_diagnostics("anchor A as anchor:\n    x: 1\n", "A");
    assert!(diagnostics.iter().any(|it| it.contains("reserved word")), "{diagnostics:?}");
}

#[test]
fn unknown_names_are_reported_where_they_are_written() {
    for (source, expected) in [
        ("anchor A extends Missing:\n    x: 1\n", "cannot find anchor `Missing`"),
        ("a:: Nonsense: 1\n", "unknown type `Nonsense`"),
        ("a: 1\na: 2\n", "already declared"),
        ("unknown-keyword Thing:\n    x: 1\n", "is not a keyword here"),
    ] {
        let (_, diagnostics) = eval_with_diagnostics(source, "a");
        assert!(
            diagnostics.iter().any(|it| it.contains(expected)),
            "expected {expected:?} for {source:?}, got {diagnostics:?}"
        );
    }
}

#[test]
fn an_anchor_may_not_inherit_from_itself() {
    let (_, diagnostics) = eval_with_diagnostics("anchor A extends A:\n    x: 1\n", "A");
    assert!(diagnostics.iter().any(|it| it.contains("inherits from itself")), "{diagnostics:?}");

    let cycle = "anchor A extends B:\n    x: 1\n\nanchor B extends A:\n    y: 2\n";
    let (_, diagnostics) = eval_with_diagnostics(cycle, "A");
    assert!(diagnostics.iter().any(|it| it.contains("inherits from itself")), "{diagnostics:?}");
}

#[test]
fn forward_references_resolve() {
    assert_eq!(eval("first: {second}\nsecond: 42\n", "first"), json!(42));
    let source = "anchor A:\n    x: {B.y}\n\nanchor B:\n    y: found\n";
    assert_eq!(eval(source, "A"), json!({ "x": "found" }));
}

#[test]
fn anchors_resolve_by_reference_and_keep_their_identity() {
    let source = "\
anchor Design:
    surface: blue

holder:
    design: {Design}

same: {holder.design.surface}
";
    assert_eq!(eval(source, "same"), json!("blue"));
    assert_eq!(eval(source, "holder"), json!({ "design": { "surface": "blue" } }));
}

#[test]
fn a_spread_beside_properties_merges_the_dictionary() {
    let base = "anchor Base:\n    settings:\n        a: 1\n        b: 2\n\n";

    // `+` merges shallowly, and the properties written beside it win.
    assert_eq!(
        eval(
            &format!("{base}anchor Child extends Base:\n    settings:\n        + {{super.settings}}\n        c: 3\n"),
            "Child"
        ),
        json!({ "settings": { "a": 1, "b": 2, "c": 3 } })
    );
    // Order decides who wins: a spread after the properties overrides them.
    assert_eq!(
        eval(
            &format!("{base}anchor Child extends Base:\n    settings:\n        b: 20\n        + {{super.settings}}\n"),
            "Child"
        ),
        json!({ "settings": { "b": 2, "a": 1 } })
    );
    // `++` merges deeply.
    let nested = "anchor Base:\n    settings:\n        deep:\n            a: 1\n\n";
    assert_eq!(
        eval(
            &format!("{nested}anchor Child extends Base:\n    settings:\n        ++ {{super.settings}}\n        deep:\n            b: 2\n"),
            "Child"
        ),
        json!({ "settings": { "deep": { "a": 1, "b": 2 } } })
    );
    // A list block with a spread still builds a list, not a merge.
    assert_eq!(
        eval(
            "anchor Base:\n    items:\n        - a\n\nanchor Child extends Base:\n    items:\n        + {super.items}\n        - b\n",
            "Child"
        ),
        json!({ "items": ["a", "b"] })
    );
}

#[test]
fn a_key_inside_a_run_of_prose_is_prose() {
    // A sentence that happens to contain a colon is a sentence. Only a blank
    // line says the author meant structure.
    let source = "\
description:
    This is a string
    and: this is still part of the string

    however:
        that: is a dictionary
";
    assert_eq!(
        eval(source, "description"),
        json!([
            "This is a string and: this is still part of the string",
            { "however": { "that": "is a dictionary" } }
        ])
    );
}

#[test]
fn a_block_of_keys_needs_no_blank_lines() {
    // Nothing opened a run of prose, so every line is read as what it looks
    // like. A dictionary written as a dictionary is unaffected by the rule.
    assert_eq!(
        eval("pure:\n    a: 1\n    b: 2\n    c: 3\n", "pure"),
        json!({ "a": 1, "b": 2, "c": 3 })
    );
}

#[test]
fn prose_stops_absorbing_keys_at_the_end_of_its_block() {
    // The run belongs to the block it was written in. A line that dedents out
    // of it is a property of the block above, not more prose.
    let source = "\
anchor Holder:
    first:
        Some prose
        and: more of it

    second: a value
";
    assert_eq!(
        eval(source, "Holder"),
        json!({ "first": "Some prose and: more of it", "second": "a value" })
    );
}

#[test]
fn a_list_marker_still_opens_a_list_inside_prose() {
    // The rule is about keys. A `- ` is a marker wherever it appears, because
    // a line that opens with one is not a sentence anybody wrote by accident.
    let source = "\
description:
    Some prose
    - one
    - two
";
    assert_eq!(eval(source, "description"), json!(["Some prose", ["one", "two"]]));
}

#[test]
fn a_comment_does_not_end_a_run_of_prose() {
    // A comment-only line is trivia everywhere else in the language, and a
    // blank line is what the rule is about.
    let source = "\
description:
    Some prose
    // an aside
    and: still prose
";
    assert_eq!(eval(source, "description"), json!("Some prose and: still prose"));
}

#[test]
fn a_key_after_a_dictionary_needs_no_blank_line() {
    // The run ended when the first key was read, so the sibling that follows
    // the nested block is a key like any other.
    let source = "\
description:
    Some prose

    first:
        a: 1
    second:
        b: 2
";
    assert_eq!(
        eval(source, "description"),
        json!(["Some prose", { "first": { "a": 1 }, "second": { "b": 2 } }])
    );
}

#[test]
fn a_problem_is_reported_once_where_it_was_made() {
    let only_missing = vec!["cannot find `Missing` in this scope".to_string()];
    let diagnostics = |source: &str| eval_with_diagnostics(source, "a").1;
    // The member access on a name that cannot be found.
    assert_eq!(diagnostics("a: {Missing.x}\n"), only_missing);
    // The operators applied to it.
    assert_eq!(diagnostics("a: {Missing + 1}\n"), only_missing);
    assert_eq!(diagnostics("a: {-Missing}\n"), only_missing);
    // The constraint checked against it.
    assert_eq!(diagnostics("a:: number: {Missing}\n"), only_missing);
    // A variable that failed, read later by another.
    assert_eq!(diagnostics("b: {Missing}\na:: number: {b.x}\n"), only_missing);
    // A property that failed, read later through `self`.
    assert_eq!(
        diagnostics(
            "anchor A:\n    p: {Missing}\n    q: {self.p.x}\n    r:: number: {self.p}\na: {A}\n"
        ),
        only_missing
    );
}

#[test]
fn a_failure_does_not_hide_a_separate_problem() {
    let diagnostics = |source: &str| eval_with_diagnostics(source, "a").1;
    // One broken property does not silence a mistake in its neighbour.
    let found = diagnostics("anchor A:\n    p: {Missing}\n    q:: number: text\na: {A}\n");
    assert_eq!(found.len(), 2, "{found:?}");
    // A problem with nothing failed underneath it is always reported.
    assert_eq!(diagnostics("a: {1 / 0}\n").len(), 1);
    assert_eq!(diagnostics("a: {-true}\n").len(), 1);
    // An anchor with a broken property is still a value its readers can use.
    let found = diagnostics("anchor A:\n    p: {Missing}\nb:: string: {A}\na: 1\n");
    assert_eq!(found.len(), 2, "the constraint on `b` is a separate mistake: {found:?}");
}

// ---- fenced code blocks -----------------------------------------------------------

#[test]
fn a_fence_keeps_its_content_exactly_as_written() {
    let source = "\
example:
    ```css
    .button {
      background-color: #00a; // not a comment
    }

    a: ${not} {an} \\escape
    ```
";
    assert_eq!(
        eval(source, "example"),
        json!("```css\n.button {\n  background-color: #00a; // not a comment\n}\n\na: ${not} {an} \\escape\n```")
    );
}

#[test]
fn a_fence_is_a_paragraph_of_its_own() {
    let source = "\
prompt:
    Compare that
    to this:
    ~~~
    one
    ~~~
    Which one
    would you rather write?

    Next paragraph.
";
    assert_eq!(
        eval(source, "prompt"),
        json!("Compare that to this:\n~~~\none\n~~~\nWhich one would you rather write?\nNext paragraph.")
    );
}

#[test]
fn a_fence_nested_in_a_list_item_keeps_its_relative_indentation() {
    let source = "\
steps:
    - Run it
        ```sh
          indented
        ```
";
    assert_eq!(eval(source, "steps"), json!(["Run it", "```sh\n  indented\n```"]));
}

#[test]
fn an_unclosed_fence_is_reported() {
    let (_, diagnostics) = eval_with_diagnostics("a:\n    ```\n    code\n", "a");
    assert!(diagnostics.iter().any(|it| it.contains("never closed")), "{diagnostics:?}");
}
