//! The plugins built into the compiler.
//!
//! A bundled plugin is the same as an installed one, except that its files are
//! embedded rather than read from disk: its Piton files in the prelude, as
//! `@piton/<name>`, and its declaration and code here.

use piton_compile::prelude;

/// A plugin built into the compiler.
pub struct Bundled {
    /// The module its declaration is embedded as.
    pub declaration: &'static str,
    /// The package its Piton files are imported from.
    pub module_root: &'static str,
    /// Its code, by path inside the plugin.
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

pub const PLUGINS: &[Bundled] = &[Bundled {
    declaration: prelude::TEST_PLUGIN_DECLARATION,
    module_root: prelude::TEST_PACKAGE,
    files: TEST_FILES,
}];
