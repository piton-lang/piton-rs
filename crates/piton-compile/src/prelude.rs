//! Built-in packages.
//!
//! The compiler contains `@piton/config` and `@piton/belay`. They resolve from
//! a virtual file system and not from disk. Thus a project has the same
//! vocabulary in all checkouts.
//!
//! Belay and the test plugin are plugins in the compiler. The compiler embeds
//! their Piton files from `plugins/belay/pi` and `plugins/test/pi`. Thus a
//! project imports the source of the plugin itself. No second copy must stay
//! the same as it. The plugin host embeds their code.

/// The module path of the project configuration package.
pub const CONFIG_PACKAGE: &str = "@piton/config";
/// The module path of the Belay framework package.
pub const BELAY_PACKAGE: &str = "@piton/belay";
/// The module path of the package-management package.
pub const PACKAGING_PACKAGE: &str = "@piton/packaging";
/// The module path of the plugin package. It holds the `piton-plugin` keyword
/// that declares a plugin.
pub const PLUGIN_PACKAGE: &str = "@piton/plugin";
/// The module path of the Piton files of the bundled test plugin. They hold the
/// `test` and `test-config` keywords.
pub const TEST_PACKAGE: &str = "@piton/test";
/// The module path of the declaration of the bundled test plugin. It is a usual
/// module. Thus the plugin host compiles it as the configuration compiles the
/// declaration of an installed plugin.
pub const TEST_PLUGIN_DECLARATION: &str = "@piton/test/plugin";
/// The module path of the declaration of the bundled migrate plugin. The plugin
/// has no Piton files. Its package path is only a name.
pub const MIGRATE_PLUGIN_DECLARATION: &str = "@piton/migrate/plugin";
/// The package path that names the migrate plugin.
pub const MIGRATE_PACKAGE: &str = "@piton/migrate";
/// The module path of the declaration of the bundled init plugin. The init
/// plugin has no Piton files, as migrate. Its package path is only a name.
pub const INIT_PLUGIN_DECLARATION: &str = "@piton/init/plugin";
/// The package path that names the init plugin.
pub const INIT_PACKAGE: &str = "@piton/init";
/// The module path of the declaration of the bundled Belay plugin.
pub const BELAY_PLUGIN_DECLARATION: &str = "@piton/belay/plugin";

/// The special export that resolves to the compiled shape root of the current
/// output target.
pub const BELAY_SHAPE: &str = "BELAY_COMPILED_SHAPE";

/// The marker that `BELAY_COMPILED_SHAPE` evaluates to.
///
/// The evaluator cannot know its value. The path depends on the adapter *and*
/// on the generated file that contains the use. Thus the evaluation makes this
/// marker. Each adapter replaces it with a path relative to the output file.
///
/// The private-use bracket characters prevent a conflict with prose. A
/// document can name `BELAY_COMPILED_SHAPE`, and no adapter rewrites it.
pub const BELAY_SHAPE_TOKEN: &str = "\u{e000}BELAY_COMPILED_SHAPE\u{e001}";

/// The four location exports that Belay resolves when it writes the output.
///
/// Each names a directory that the generated artifact must refer to. The
/// evaluator cannot know them. The agent root depends on the adapter of the
/// compilation. All four are relative to the file that contains the use. Thus
/// the evaluation gives a marker, as for `BELAY_COMPILED_SHAPE`. Belay then
/// puts a path for each file in place of the marker.
pub const BELAY_AGENT_ROOT: &str = "BELAY_AGENT_ROOT";
pub const BELAY_PROJECT_ROOT: &str = "BELAY_PROJECT_ROOT";
pub const BELAY_SHAPE_ROOT: &str = "BELAY_SHAPE_ROOT";
pub const BELAY_CODE_ROOT: &str = "BELAY_CODE_ROOT";

/// The marker that each of the four location exports evaluates to.
pub const BELAY_AGENT_ROOT_TOKEN: &str = "\u{e000}BELAY_AGENT_ROOT\u{e001}";
pub const BELAY_PROJECT_ROOT_TOKEN: &str = "\u{e000}BELAY_PROJECT_ROOT\u{e001}";
pub const BELAY_SHAPE_ROOT_TOKEN: &str = "\u{e000}BELAY_SHAPE_ROOT\u{e001}";
pub const BELAY_CODE_ROOT_TOKEN: &str = "\u{e000}BELAY_CODE_ROOT\u{e001}";

/// Each location export, with the marker that it evaluates to. Thus a consumer
/// can iterate over the set. It does not need to repeat the five names.
pub const BELAY_LOCATION_MARKERS: [(&str, &str); 5] = [
    (BELAY_SHAPE, BELAY_SHAPE_TOKEN),
    (BELAY_AGENT_ROOT, BELAY_AGENT_ROOT_TOKEN),
    (BELAY_PROJECT_ROOT, BELAY_PROJECT_ROOT_TOKEN),
    (BELAY_SHAPE_ROOT, BELAY_SHAPE_ROOT_TOKEN),
    (BELAY_CODE_ROOT, BELAY_CODE_ROOT_TOKEN),
];

/// Placeholders in the package template. The load replaces them with the real markers.
const MARKER_PLACEHOLDERS: [(&str, &str); 5] = [
    ("__BELAY_SHAPE_MARKER__", BELAY_SHAPE_TOKEN),
    ("__BELAY_AGENT_ROOT_MARKER__", BELAY_AGENT_ROOT_TOKEN),
    ("__BELAY_PROJECT_ROOT_MARKER__", BELAY_PROJECT_ROOT_TOKEN),
    ("__BELAY_SHAPE_ROOT_MARKER__", BELAY_SHAPE_ROOT_TOKEN),
    ("__BELAY_CODE_ROOT_MARKER__", BELAY_CODE_ROOT_TOKEN),
];

/// `@piton/config`. It holds the anchors of a `piton.config.pi` file.
pub const CONFIG_SOURCE: &str = r#"// Bundled with the Piton compiler.

export abstract anchor PitonConfig as piton-config:
    root:: string
    entry:: string:: null: null
    output:: string:: null: null
    renderer:: string:: null: null
    packages:: list:: null: null
    dependencies:: list:: null: null
    plugins:: list:: null: null
"#;

/// `@piton/packaging`. It holds the anchor that declares a package to publish.
///
/// A package declaration tells which part of this project to publish. It also
/// tells what that part needs. It is abstract for the same reason as the
/// configuration anchors. The specification uses the form
/// `piton-package MyThing:`. A keyword has this form only when its anchor is a
/// shape and not a value.
///
/// `name` gives a package a name that an anchor cannot have, such as
/// `my-package`. Without `name`, the name of the anchor is the package name.
/// In both cases, the name cannot contain `/` or start with `@`.
///
/// `dependencies` is a list and not `string[]`. The reason is that a pin
/// dictionary can follow a URL.
pub const PACKAGING_SOURCE: &str = r#"// Bundled with the Piton compiler.

export abstract anchor PitonPackage as piton-package:
    name:: string:: null: null
    root:: string
    dependencies:: list:: null: null
"#;

// The definitions of the framework. They come from the specification that
// describes them. If one of these files moves, the build fails clearly. That is
// the purpose. The compiler and the specification cannot have different skills.
const BELAY_CONSTRUCT: &str = include_str!("../../../plugins/belay/pi/Construct.pi");
const BELAY_INSTRUCTION: &str = include_str!("../../../plugins/belay/pi/Instruction.pi");
const BELAY_SKILL: &str = include_str!("../../../plugins/belay/pi/Skill.pi");
const BELAY_COMMAND: &str = include_str!("../../../plugins/belay/pi/Command.pi");
const BELAY_AGENT: &str = include_str!("../../../plugins/belay/pi/Agent.pi");
// The adapters. They also come from the specification. A project selects an
// adapter when it lists the anchor. Belay reads the paths and settings of the
// target from it.
const BELAY_ADAPTER: &str = include_str!("../../../plugins/belay/pi/adapters/Adapter.pi");
const BELAY_CLAUDE_CODE: &str = include_str!("../../../plugins/belay/pi/adapters/ClaudeCode.pi");
const BELAY_CODEX: &str = include_str!("../../../plugins/belay/pi/adapters/Codex.pi");
const BELAY_OPENCODE: &str = include_str!("../../../plugins/belay/pi/adapters/OpenCode.pi");
const BELAY_MUSE: &str = include_str!("../../../plugins/belay/pi/adapters/Muse.pi");

// `@piton/plugin`. It comes from the specification that describes it, as the
// constructs of Belay do.
const PLUGIN_ANCHOR: &str = include_str!("../../../spec/scope/plugins/anchors/Plugin.pi");

// The Piton files and the declaration of the bundled test plugin. The plugin
// host embeds its behaviour, in TypeScript.
const TEST_INDEX: &str = include_str!("../../../plugins/test/pi/index.pi");
const TEST_TEST: &str = include_str!("../../../plugins/test/pi/Test.pi");
const TEST_CONFIG: &str = include_str!("../../../plugins/test/pi/TestConfig.pi");
const TEST_PLUGIN: &str = include_str!("../../../plugins/test/plugin.pi");
const BELAY_PLUGIN: &str = include_str!("../../../plugins/belay/plugin.pi");
const MIGRATE_PLUGIN: &str = include_str!("../../../plugins/migrate/plugin.pi");
const INIT_PLUGIN: &str = include_str!("../../../plugins/init/plugin.pi");

/// `@piton/belay`. It exports the constructs and the adapters again. It adds
/// the configuration anchor and the special imports.
const BELAY_INDEX: &str = include_str!("../../../plugins/belay/pi/index.pi");

/// All modules of the bundled packages, by virtual path.
///
/// The paths have no extension, as a resolved module path has none. Thus
/// `from ./anchors/Skill export Skill` in the package resolves exactly as on
/// disk.
fn modules() -> &'static [(&'static str, &'static str)] {
    static MODULES: std::sync::OnceLock<Vec<(&'static str, &'static str)>> =
        std::sync::OnceLock::new();
    MODULES
        .get_or_init(|| {
            let mut text = BELAY_INDEX.to_string();
            for (placeholder, marker) in MARKER_PLACEHOLDERS {
                text = text.replace(placeholder, marker);
            }
            let index: &'static str = Box::leak(text.into_boxed_str());
            vec![
                (CONFIG_PACKAGE, CONFIG_SOURCE),
                (PACKAGING_PACKAGE, PACKAGING_SOURCE),
                (BELAY_PACKAGE, index),
                ("@piton/belay/Construct", BELAY_CONSTRUCT),
                ("@piton/belay/Instruction", BELAY_INSTRUCTION),
                ("@piton/belay/Skill", BELAY_SKILL),
                ("@piton/belay/Command", BELAY_COMMAND),
                ("@piton/belay/Agent", BELAY_AGENT),
                ("@piton/belay/adapters/Adapter", BELAY_ADAPTER),
                ("@piton/belay/adapters/ClaudeCode", BELAY_CLAUDE_CODE),
                ("@piton/belay/adapters/Codex", BELAY_CODEX),
                ("@piton/belay/adapters/OpenCode", BELAY_OPENCODE),
                ("@piton/belay/adapters/Muse", BELAY_MUSE),
                (PLUGIN_PACKAGE, PLUGIN_ANCHOR),
                (TEST_PACKAGE, TEST_INDEX),
                ("@piton/test/Test", TEST_TEST),
                ("@piton/test/TestConfig", TEST_CONFIG),
                (TEST_PLUGIN_DECLARATION, TEST_PLUGIN),
                (BELAY_PLUGIN_DECLARATION, BELAY_PLUGIN),
                (MIGRATE_PLUGIN_DECLARATION, MIGRATE_PLUGIN),
                (INIT_PLUGIN_DECLARATION, INIT_PLUGIN),
            ]
        })
        .as_slice()
}

/// Returns the source of a bundled module, if `path` names a bundled module.
pub fn package_source(path: &str) -> Option<&'static str> {
    modules()
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, source)| *source)
}

/// True when `path` names a module in a bundled package.
pub fn is_package(path: &str) -> bool {
    package_source(path).is_some()
}

/// True when `path` is in a bundled package, if it names a module or not.
/// Callers use it to keep package internals out of reports for users.
pub fn is_package_path(path: &str) -> bool {
    path.starts_with('@')
}

/// The packages that a project can import by name.
pub const PACKAGE_ROOTS: [&str; 5] =
    [CONFIG_PACKAGE, BELAY_PACKAGE, PACKAGING_PACKAGE, PLUGIN_PACKAGE, TEST_PACKAGE];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_constructs_come_from_the_specification() {
        // This is not a copy. It is the same bytes as the specification.
        let skill = package_source("@piton/belay/Skill").expect("module");
        assert!(
            skill.contains("abstract anchor Skill extends Construct as skill"),
            "{skill}"
        );
        assert_eq!(skill, BELAY_SKILL);
    }

    #[test]
    fn every_package_module_is_addressable() {
        for (path, source) in modules() {
            assert!(is_package(path), "{path}");
            assert!(is_package_path(path), "{path}");
            assert!(!source.is_empty(), "{path}");
        }
    }

    #[test]
    fn every_location_marker_is_substituted_once() {
        let index = package_source(BELAY_PACKAGE).expect("index");
        for (name, marker) in BELAY_LOCATION_MARKERS {
            assert!(index.contains(marker), "{name} has no marker in the package");
        }
        for (placeholder, _) in MARKER_PLACEHOLDERS {
            assert!(
                !index.contains(placeholder),
                "{placeholder} survived substitution"
            );
        }
    }

    #[test]
    fn location_markers_are_distinct() {
        for (index, (name, marker)) in BELAY_LOCATION_MARKERS.iter().enumerate() {
            for (other_name, other) in &BELAY_LOCATION_MARKERS[index + 1..] {
                assert_ne!(marker, other, "{name} and {other_name} share a marker");
            }
        }
    }
}
