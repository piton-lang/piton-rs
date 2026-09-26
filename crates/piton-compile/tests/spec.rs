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
    // Inherited properties keep the base's order; the child's own come after.
    assert_eq!(
        &names[..4],
        &[
            "what-is-a-type",
            "supportedOperators",
            "unsupportedOperators",
            "description"
        ]
    );

    let dictionaries = compilation.find_anchor("Dictionaries").expect("Dictionaries");
    let names: Vec<&str> = compilation.store().anchor(dictionaries).slots.keys().map(String::as_str).collect();
    assert_eq!(names.last(), Some(&"hyphenatedKeys"), "child-only properties append");
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
        &["description", "targetId", "instructionFile", "referenceRoot", "documentationChecked", "requirements"]
    );
    let properties = &compilation.store().anchor(claude).properties;
    assert_eq!(
        properties.get("targetId"),
        Some(&Value::string("claude-code"))
    );
}

#[test]
fn the_framework_source_is_not_reported_dead() {
    // `prelude` compiles `spec/scope/belay/anchors/*.pi` into the binary with
    // `include_str!`, so this project holds each of those files twice: once on
    // disk and once as the `@piton/belay` module it became. Everything imports
    // the package copy, which leaves the file on disk reached by nothing --
    // and it is the opposite of dead. It is the framework. A report that names
    // it unreachable is inviting someone to delete the compiler's vocabulary.
    let compilation = compile_spec();
    let reachability = piton_compile::reach::from_entry(&compilation);

    let dead: Vec<String> = reachability
        .unreachable
        .iter()
        .map(|anchor| compilation.anchor_module_path(*anchor))
        .chain(reachability.unreachable_modules.iter().cloned())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .filter(|path| path.contains("scope/belay/anchors/"))
        .collect();

    assert!(
        dead.is_empty(),
        "these are embedded in the compiler and reached through `@piton/belay`: {dead:?}"
    );
}

#[test]
fn a_module_that_only_re_exports_is_not_dead() {
    // `spec/agent/index.pi` and `spec/scope/belay/index.pi` declare no anchor
    // of their own; they exist to hand on what another file declares. A walk
    // that counts only declarations finds nothing in them.
    let compilation = compile_spec();
    let reachability = piton_compile::reach::from_entry(&compilation);

    let dead: Vec<String> = reachability
        .unreachable_modules
        .iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect();

    for barrel in ["spec/agent/index.pi", "spec/scope/belay/index.pi"] {
        assert!(
            !dead.iter().any(|path| path.ends_with(barrel)),
            "`{barrel}` carries the whole import chain: {dead:?}"
        );
    }
}
