//! Compiles the specification, which is itself written in Piton.

use std::path::{Path, PathBuf};

use piton_compile::{config, Compilation};
use piton_core::{diagnostics, Value};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn compile_spec() -> Compilation {
    let root = repo_root();
    let (project, diagnostics) = config::load(&root, None);
    assert!(!diagnostics.has_errors(), "config: {:#?}", diagnostics.as_slice());
    Compilation::build(project)
}

#[test]
fn the_specification_compiles() {
    let compilation = compile_spec();
    let root = repo_root();
    let errors: Vec<String> = compilation
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| diagnostics::render(d, compilation.source_of(&d.file), Some(&root)))
        .collect();
    assert!(errors.is_empty(), "{} errors:\n{}", errors.len(), errors.join("\n"));
}

#[test]
fn inherited_properties_keep_the_base_order() {
    let compilation = compile_spec();
    let strings = compilation.find_anchor("Strings").expect("Strings");
    let names: Vec<&str> = compilation.store().anchor(strings).slots.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        vec![
            "whatIsAType",
            "supportedOperators",
            "unsupportedOperators",
            "description"
        ]
    );

    let dictionaries = compilation.find_anchor("Dictionaries").expect("Dictionaries");
    let names: Vec<&str> = compilation.store().anchor(dictionaries).slots.keys().map(String::as_str).collect();
    assert_eq!(names.last(), Some(&"validKeys"), "child-only properties append");
}

#[test]
fn self_resolves_to_the_derived_anchor() {
    let compilation = compile_spec();
    let addition = compilation.find_anchor("AdditionOperator").expect("AdditionOperator");
    let value = compilation.store().anchor(addition).properties.get("orderOfOperations").expect("orderOfOperations");
    let Value::Str(text) = value else { panic!("expected a string, got {value:?}") };
    let rendered = text.as_plain().expect("plain");
    assert!(
        rendered.starts_with("ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator."),
        "got: {rendered}"
    );
    assert!(rendered.contains('\n'), "the blank line is an explicit break");
}

#[test]
fn adapters_inherit_the_shared_contract() {
    let compilation = compile_spec();
    let claude = compilation.find_anchor("ClaudeCodeAdapter").expect("ClaudeCodeAdapter");
    let names: Vec<&str> = compilation.store().anchor(claude).slots.keys().map(String::as_str).collect();
    assert_eq!(
        &names[..6],
        &["description", "targetId", "instructionFile", "referenceRoot", "status", "documentationChecked"]
    );
    let properties = &compilation.store().anchor(claude).properties;
    assert_eq!(
        properties.get("targetId"),
        Some(&Value::string("claude-code"))
    );
}
