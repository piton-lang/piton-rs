//! The plugins in the compiler.
//!
//! A bundled plugin is the same as an installed plugin, but the compiler
//! embeds its files. The compiler does not read them from disk. Its Piton files are
//! in the prelude, as `@piton/<name>`. Its declaration and code are here.

use piton_compile::prelude;

/// A plugin in the compiler.
pub struct Bundled {
    /// The embedded module of its declaration.
    pub declaration: &'static str,
    /// The package that holds its Piton files.
    pub module_root: &'static str,
    /// Its code, by path in the plugin.
    pub files: &'static [(&'static str, &'static str)],
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

/// The init plugin. It gives `piton init` and its templates. `build.rs`
/// embeds each file below `plugins/init/src` and `plugins/init/templates`.
const INIT_FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/init_files.rs"));

pub const PLUGINS: &[Bundled] = &[
    Bundled {
        declaration: prelude::BELAY_PLUGIN_DECLARATION,
        module_root: prelude::BELAY_PACKAGE,
        files: BELAY_FILES,
    },
    Bundled {
        declaration: prelude::INIT_PLUGIN_DECLARATION,
        module_root: prelude::INIT_PACKAGE,
        files: INIT_FILES,
    },
    Bundled {
        declaration: prelude::MIGRATE_PLUGIN_DECLARATION,
        module_root: prelude::MIGRATE_PACKAGE,
        files: MIGRATE_FILES,
    },
    Bundled {
        declaration: prelude::TEST_PLUGIN_DECLARATION,
        module_root: prelude::TEST_PACKAGE,
        files: TEST_FILES,
    },
];
