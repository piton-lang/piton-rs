//! The plugins in the compiler.
//!
//! A bundled plugin is the same as an installed plugin, but the compiler
//! embeds its files. The compiler does not read them from disk. Its Piton files are
//! bundled packages, as `@piton/<name>`. The prelude of the compiler holds the
//! Piton files of Belay and the test plugin. A bundled plugin that the
//! compiler does not know gives its Piton files and its declaration in
//! `modules`. `register` adds them to the bundled packages.

use piton_compile::prelude;

/// A plugin in the compiler.
pub struct Bundled {
    /// The embedded module of its declaration.
    pub declaration: &'static str,
    /// The package that holds its Piton files.
    pub module_root: &'static str,
    /// Its code, by path in the plugin.
    pub files: &'static [(&'static str, &'static str)],
    /// Its declaration and Piton files, by module path, when the prelude of
    /// the compiler does not hold them.
    pub modules: &'static [(&'static str, &'static str)],
}

/// The module path of the Piton files of the Emul plugin. They hold the
/// `emul-design` keyword.
pub const EMUL_PACKAGE: &str = "@piton/emul";
/// The module path of the declaration of the Emul plugin.
pub const EMUL_PLUGIN_DECLARATION: &str = "@piton/emul/plugin";

/// Adds the Piton files and the declarations of the bundled plugins to the
/// bundled packages of the compiler. A second call does nothing. Call it
/// before a compile that can import them.
pub fn register() {
    for plugin in PLUGINS.iter().filter(|plugin| !plugin.modules.is_empty()) {
        piton_compile::prelude::register_package(plugin.module_root, plugin.modules);
    }
}

/// The test plugin: `piton test`.
const TEST_FILES: &[(&str, &str)] = &[
    ("src/index.ts", include_str!("../../../plugins/test/src/index.ts")),
    ("src/tags.ts", include_str!("../../../plugins/test/src/tags.ts")),
    ("src/output.ts", include_str!("../../../plugins/test/src/output.ts")),
    ("src/progress.ts", include_str!("../../../plugins/test/src/progress.ts")),
    ("src/judge.ts", include_str!("../../../plugins/test/src/judge.ts")),
];

/// Belay is the framework for agent guidance. `belay_files.rs` lists its
/// files. The port keeps that list the same as `plugins/belay/src`.
const BELAY_FILES: &[(&str, &str)] = include!("belay_files.rs");

/// The migrate plugin. It finds old projects and offers to update them.
const MIGRATE_FILES: &[(&str, &str)] = &[
    ("src/index.ts", include_str!("../../../plugins/migrate/src/index.ts")),
    ("src/migrations.ts", include_str!("../../../plugins/migrate/src/migrations.ts")),
];

/// The Emul plugin. Its module hook gives the module of each `.emul` file
/// that a project imports. It also adds `piton emul`.
const EMUL_FILES: &[(&str, &str)] = &[
    ("src/index.ts", include_str!("../../../plugins/emul/src/index.ts")),
    ("src/designs.ts", include_str!("../../../plugins/emul/src/designs.ts")),
];

/// The declaration and the Piton files of the Emul plugin.
const EMUL_MODULES: &[(&str, &str)] = &[
    (EMUL_PACKAGE, include_str!("../../../plugins/emul/pi/index.pi")),
    ("@piton/emul/EmulDesign", include_str!("../../../plugins/emul/pi/EmulDesign.pi")),
    (EMUL_PLUGIN_DECLARATION, include_str!("../../../plugins/emul/plugin.pi")),
];

/// The init plugin. It gives `piton init` and its templates. `build.rs`
/// embeds each file below `plugins/init/src` and `plugins/init/templates`.
const INIT_FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/init_files.rs"));

pub const PLUGINS: &[Bundled] = &[
    Bundled {
        declaration: prelude::BELAY_PLUGIN_DECLARATION,
        module_root: prelude::BELAY_PACKAGE,
        files: BELAY_FILES,
        modules: &[],
    },
    Bundled {
        declaration: EMUL_PLUGIN_DECLARATION,
        module_root: EMUL_PACKAGE,
        files: EMUL_FILES,
        modules: EMUL_MODULES,
    },
    Bundled {
        declaration: prelude::INIT_PLUGIN_DECLARATION,
        module_root: prelude::INIT_PACKAGE,
        files: INIT_FILES,
        modules: &[],
    },
    Bundled {
        declaration: prelude::MIGRATE_PLUGIN_DECLARATION,
        module_root: prelude::MIGRATE_PACKAGE,
        files: MIGRATE_FILES,
        modules: &[],
    },
    Bundled {
        declaration: prelude::TEST_PLUGIN_DECLARATION,
        module_root: prelude::TEST_PACKAGE,
        files: TEST_FILES,
        modules: &[],
    },
];
