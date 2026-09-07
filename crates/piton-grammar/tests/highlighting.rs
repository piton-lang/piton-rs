//! Does the TextMate grammar actually highlight Piton correctly?
//!
//! The grammar is a pile of regular expressions, so the only way to know is to
//! run them. Every pattern is compiled, every `include` is checked to point at
//! a real rule, and the patterns that carry the language's awkward cases —
//! comments that must not eat URLs, operators that must not eat hyphens — are
//! exercised against inputs taken from the specification.

use fancy_regex::Regex;
use piton_grammar::{GrammarSource, Vocabulary};
use serde_json::Value;

fn grammar() -> Value {
    let vocabulary = Vocabulary::from_compiler()
        .with_framework(vec!["agent".to_string(), "skill".to_string()], vec!["@".to_string()]);
    let files = piton_grammar::generate(&vocabulary, &GrammarSource::default());
    let file = files
        .iter()
        .find(|file| file.path.ends_with("shared/piton.tmLanguage.json"))
        .expect("the TextMate grammar is generated");
    serde_json::from_str(&file.contents).expect("the grammar is valid JSON")
}

/// Every `match`, `begin`, and `end` in the grammar, with the rule it came from.
fn patterns(grammar: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let repository = grammar["repository"].as_object().expect("a repository");
    for (name, rule) in repository {
        collect(name, rule, &mut out);
    }
    out
}

fn collect(name: &str, rule: &Value, out: &mut Vec<(String, String)>) {
    for key in ["match", "begin", "end"] {
        if let Some(pattern) = rule[key].as_str() {
            out.push((format!("{name}.{key}"), pattern.to_string()));
        }
    }
    if let Some(nested) = rule["patterns"].as_array() {
        for (index, inner) in nested.iter().enumerate() {
            collect(&format!("{name}[{index}]"), inner, out);
        }
    }
}

/// Find one named rule's pattern.
fn pattern(grammar: &Value, name: &str, key: &str) -> Regex {
    let raw = grammar["repository"][name][key]
        .as_str()
        .unwrap_or_else(|| panic!("no {name}.{key} in the grammar"));
    Regex::new(raw).unwrap_or_else(|error| panic!("{name}.{key} does not compile: {error}"))
}

fn matches(regex: &Regex, text: &str) -> bool {
    regex.is_match(text).unwrap_or(false)
}

#[test]
fn every_pattern_compiles() {
    let grammar = grammar();
    let patterns = patterns(&grammar);
    assert!(patterns.len() > 10, "expected a grammar with real rules, found {}", patterns.len());
    for (name, raw) in patterns {
        Regex::new(&raw).unwrap_or_else(|error| panic!("{name} does not compile: {error}\n{raw}"));
    }
}

#[test]
fn every_include_points_at_a_rule_that_exists() {
    let grammar = grammar();
    let repository = grammar["repository"].as_object().unwrap();
    let mut stack: Vec<&Value> = vec![&grammar];
    let mut seen = 0;
    while let Some(node) = stack.pop() {
        if let Some(include) = node["include"].as_str() {
            let name = include.strip_prefix('#').unwrap_or(include);
            assert!(repository.contains_key(name), "`{include}` is not in the repository");
            seen += 1;
        }
        for key in ["patterns"] {
            if let Some(list) = node[key].as_array() {
                stack.extend(list.iter());
            }
        }
        if let Some(object) = node.as_object() {
            for value in object.values() {
                if value.is_object() {
                    stack.push(value);
                }
            }
        }
    }
    assert!(seen > 5, "expected the grammar to use includes, saw {seen}");
}

#[test]
fn the_grammar_declares_the_language_it_is_for() {
    let grammar = grammar();
    assert_eq!(grammar["scopeName"], "source.piton");
    assert_eq!(grammar["fileTypes"][0], "pi");
}

#[test]
fn a_comment_starts_at_a_word_boundary_and_not_inside_a_url() {
    let grammar = grammar();
    let comment = pattern(&grammar, "comment", "match");
    assert!(matches(&comment, "// a whole line"));
    assert!(matches(&comment, "x: 1 // trailing"));
    assert!(matches(&comment, "    // indented"));
    // The rule that makes prose usable: a URL is not a comment.
    assert!(!matches(&comment, "url: https://example.com/a//b"));
    assert!(!matches(&comment, "x: a//b"));
}

#[test]
fn a_property_key_is_recognised_and_prose_is_not() {
    let grammar = grammar();
    let property = pattern(&grammar, "property", "begin");
    for key in ["name: value", "    name: value", "name:", "name:: string: 1", "kebab-key: v"] {
        assert!(matches(&property, key), "{key:?} should be a key");
    }
    for prose in ["Note that something", "  just some prose", "- a list item"] {
        assert!(!matches(&property, prose), "{prose:?} is not a key");
    }
}

#[test]
fn numbers_are_recognised_only_when_they_stand_alone() {
    let grammar = grammar();
    let number = pattern(&grammar, "number", "match");
    for good in ["42", "3.14", "1_200_000.00", "x: 42"] {
        assert!(matches(&number, good), "{good:?} contains a number");
    }
    // A leading dot is not a decimal, and a version inside a word is not one.
    assert!(!matches(&number, ".14"));
    assert!(!matches(&number, "v2rc"));
}

#[test]
fn an_operator_needs_space_around_it_so_hyphens_survive() {
    let grammar = grammar();
    let operator = pattern(&grammar, "operator", "match");
    for arithmetic in ["3.14 - 3.14", "a + b", "[1] ++ [2]", "x && y"] {
        assert!(matches(&operator, arithmetic), "{arithmetic:?} has an operator");
    }
    for prose in ["a well-known thing", "e.g. this", "kebab-case-name"] {
        assert!(!matches(&operator, prose), "{prose:?} has no operator");
    }
}

#[test]
fn every_interpolation_sigil_opens_an_embedded_region() {
    let grammar = grammar();
    let interpolation = pattern(&grammar, "interpolation", "begin");
    for good in ["${name}", "@{Anchor}", "reference{Other}", "{bare}"] {
        assert!(matches(&interpolation, good), "{good:?} opens an interpolation");
    }
    // A price is not a sigil.
    assert!(!matches(&interpolation, "This costs $5 plus tax"));
}

#[test]
fn declarations_capture_their_names() {
    let grammar = grammar();
    let anchor = pattern(&grammar, "anchor-declaration", "match");
    for good in [
        "anchor A:",
        "export anchor A:",
        "abstract anchor A:",
        "export abstract anchor A extends B, C as kw:",
    ] {
        assert!(matches(&anchor, good), "{good:?} is an anchor declaration");
    }
    assert!(!matches(&anchor, "anchors are useful"));

    let keyword = pattern(&grammar, "keyword-declaration", "match");
    assert!(matches(&keyword, "skill BuildThing:"));
    assert!(matches(&keyword, "export agent Reviewer extends Base:"));

    let import = pattern(&grammar, "import", "match");
    for good in ["from ./a import B", "from @piton/belay import Agent", "use ./keywords"] {
        assert!(matches(&import, good), "{good:?} is an import");
    }
}

#[test]
fn constants_and_types_come_from_the_compiler() {
    let grammar = grammar();
    let constant = pattern(&grammar, "constant", "match");
    for literal in ["true", "false", "null"] {
        assert!(matches(&constant, &format!("x: {literal}")), "{literal} is a literal");
    }
    // Part of a word is not a literal.
    assert!(!matches(&constant, "x: truest"));
    assert!(!matches(&constant, "x: nullable"));

    let annotation = pattern(&grammar, "type-annotation", "begin");
    assert!(matches(&annotation, "x:: string"));
    for builtin in piton_syntax::kind::BUILTIN_TYPES {
        let types = pattern(&grammar, "type-annotation", "begin");
        assert!(matches(&types, &format!("x:: {builtin}")), "{builtin} is a built-in type");
    }
}

#[test]
fn the_language_configuration_is_usable() {
    let vocabulary = Vocabulary::from_compiler();
    let files = piton_grammar::generate(&vocabulary, &GrammarSource::default());
    let configuration = files
        .iter()
        .find(|file| file.path.ends_with("shared/language-configuration.json"))
        .expect("a language configuration is generated");
    let value: Value = serde_json::from_str(&configuration.contents).expect("valid JSON");
    assert_eq!(value["comments"]["lineComment"], "//");
    // Every indentation rule has to be a usable regular expression.
    for key in ["increaseIndentPattern", "decreaseIndentPattern"] {
        let raw = value["indentationRules"][key].as_str().expect(key);
        Regex::new(raw).unwrap_or_else(|error| panic!("{key}: {error}"));
    }
    let increase = Regex::new(value["indentationRules"]["increaseIndentPattern"].as_str().unwrap())
        .unwrap();
    assert!(matches(&increase, "anchor A:"), "a declaration opens a block");
    assert!(matches(&increase, "    items:"), "so does a property with no value");
    assert!(!matches(&increase, "    name: value"), "a property with a value does not");
}
