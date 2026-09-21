//! Checks that the bundled `@piton/belay` package is the specification.
//!
//! The four constructs are not written in Rust. `spec/scope/belay/anchors/*.pi`
//! is embedded into the compiler at build time and served from a virtual
//! filesystem, so the specification's description of a skill is the skill a
//! project actually gets.
//!
//! These tests hold that arrangement in place. One checks the shape the
//! compiler serves against the shape the file on disk declares; the other checks
//! that each construct arrives through its keyword, since a construct with no
//! alias cannot be used whatever its shape.

use std::path::{Path, PathBuf};

use piton_compile::{Compilation, Project, Symbol};
use piton_core::AnchorId;

/// The four agentic constructs, plus the base they share.
const CONSTRUCTS: &[&str] = &["Construct", "Instruction", "Skill", "Command", "Agent"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

struct Sandbox {
    dir: PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Sandbox {
    fn new() -> Sandbox {
        // The thread id keeps parallel tests in this binary from sharing a
        // directory and deleting it out from under each other.
        let dir = std::env::temp_dir().join(format!(
            "piton-framework-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        Sandbox { dir }
    }
}

/// Compiles a file that pulls every construct out of the bundled package.
fn bundled(sandbox: &Sandbox) -> Compilation {
    let entry = sandbox.dir.join("main.pi");
    std::fs::write(
        &entry,
        format!("from @piton/belay import\n    {}\n", CONSTRUCTS.join(",\n    ")),
    )
    .expect("write");
    let mut project = Project::for_file(&entry);
    project.source_root = sandbox.dir.clone();
    project.root = sandbox.dir.clone();
    Compilation::build(project)
}

/// Compiles the specification's own description of one construct.
fn specified(name: &str) -> Compilation {
    let root = repo_root();
    let entry = root.join(format!("spec/scope/belay/anchors/{name}.pi"));
    let mut project = Project::for_file(&entry);
    project.source_root = root.join("spec");
    project.root = root;
    Compilation::build(project)
}

/// A comparable rendering of an anchor's declared shape.
///
/// Values are left out on purpose: an abstract construct declares a shape, and
/// it is the shape that has to agree.
fn shape(compilation: &Compilation, anchor: AnchorId) -> String {
    let def = compilation.store().anchor(anchor);
    let mut out = String::new();
    if def.is_abstract {
        out.push_str("abstract ");
    }
    out.push_str(&def.name);
    if let Some(alias) = &def.alias {
        out.push_str(&format!(" as {alias}"));
    }
    let bases: Vec<&str> = def
        .bases
        .iter()
        .map(|base| compilation.store().anchor(*base).name.as_str())
        .collect();
    if !bases.is_empty() {
        out.push_str(&format!(" extends {}", bases.join(", ")));
    }
    out.push('\n');

    for (name, slot) in &def.slots {
        let constraints: Vec<String> = slot
            .constraints
            .iter()
            .map(piton_compile::eval::constraint_label)
            .collect();
        out.push_str(&format!(
            "    {name}:: {}{}\n",
            if constraints.is_empty() {
                "any".to_string()
            } else {
                constraints.join(":: ")
            },
            if slot.has_value { " = <value>" } else { "" }
        ));
    }
    out
}

fn find(compilation: &Compilation, name: &str) -> AnchorId {
    compilation
        .find_anchor(name)
        .unwrap_or_else(|| panic!("`{name}` not found"))
}

#[test]
fn the_bundled_package_matches_the_specification() {
    let sandbox = Sandbox::new();
    let bundled = bundled(&sandbox);
    assert!(
        !bundled.has_errors(),
        "the bundled package must compile: {:#?}",
        bundled.diagnostics.as_slice()
    );

    let mut differences = Vec::new();
    for name in CONSTRUCTS {
        let described = specified(name);
        assert!(
            !described.has_errors(),
            "`spec/scope/belay/anchors/{name}.pi` must compile: {:#?}",
            described.diagnostics.as_slice()
        );

        let shipped = shape(&bundled, find(&bundled, name));
        let documented = shape(&described, find(&described, name));
        if shipped != documented {
            differences.push(format!(
                "{name}\n  compiler serves:\n{}\n  the file declares:\n{}",
                indent(&shipped),
                indent(&documented)
            ));
        }
    }

    assert!(
        differences.is_empty(),
        "the bundled `@piton/belay` no longer matches the files it embeds:\n\n{}",
        differences.join("\n\n")
    );
}

#[test]
fn the_constructs_are_served_from_the_specification_files() {
    // The shape matching above would still pass if someone reintroduced a
    // hand-written copy. This is what makes that impossible: the module a
    // construct arrives in has to be the embedded specification file, and its
    // text has to be what is on disk.
    let sandbox = Sandbox::new();
    let bundled = bundled(&sandbox);
    let root = repo_root();

    for name in CONSTRUCTS {
        let anchor = find(&bundled, name);
        let module = bundled.anchor_module_path(anchor);
        assert_eq!(
            module,
            PathBuf::from(format!("@piton/belay/anchors/{name}")),
            "`{name}` must come from the embedded specification file"
        );

        let served = bundled
            .source_of(&module)
            .unwrap_or_else(|| panic!("no source for {name}"));
        let on_disk = std::fs::read_to_string(
            root.join(format!("spec/scope/belay/anchors/{name}.pi")),
        )
        .expect("readable");
        assert_eq!(
            served, on_disk,
            "`{name}` is served from a copy rather than from the file itself"
        );
    }
}

#[test]
fn the_package_exports_the_framework_and_not_its_documentation() {
    // The construct files also hold the prose that documents them. Those
    // anchors are reachable inside the package but must not leak out of it, or
    // `from @piton/belay import SkillBehavior` would start working.
    let sandbox = Sandbox::new();
    let bundled = bundled(&sandbox);
    let module = bundled
        .graph()
        .id_for(Path::new("@piton/belay"))
        .expect("the package is loaded");

    let exported = bundled.resolution.exported_names(module);
    for name in CONSTRUCTS {
        assert!(exported.contains(&name.to_string()), "{name} is not exported");
    }
    for documentation in [
        "SkillBehavior",
        "AgentBehavior",
        "CommandBehavior",
        "InstructionBehavior",
        "FrameworkSpecification",
    ] {
        assert!(
            !exported.contains(&documentation.to_string()),
            "`{documentation}` is documentation and must stay inside the package"
        );
    }
}

#[test]
fn every_construct_is_reachable_through_its_keyword() {
    let sandbox = Sandbox::new();
    let bundled = bundled(&sandbox);
    let module = bundled
        .graph()
        .id_for(Path::new("@piton/belay"))
        .expect("the package is loaded");

    // A construct is used by writing its keyword, so a missing alias makes the
    // construct unusable no matter what its shape is.
    for (name, keyword) in [
        ("Instruction", "instruction"),
        ("Skill", "skill"),
        ("Command", "command"),
        ("Agent", "agent"),
    ] {
        let Some(Symbol::Anchor(anchor)) =
            bundled
                .resolution
                .lookup_export(module, name, &mut Default::default())
        else {
            panic!("`{name}` is not exported by @piton/belay");
        };
        assert_eq!(
            bundled.store().anchor(anchor).alias.as_deref(),
            Some(keyword),
            "`{name}` must be usable as `{keyword}`"
        );
    }
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
