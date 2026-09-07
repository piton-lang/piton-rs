//! Modules built into the compiler itself.
//!
//! `@piton/config` is always available so that a project can describe itself
//! in Piton rather than in a second configuration language.

use crate::framework::VirtualModule;

/// The module name that provides the project configuration anchors.
pub const CONFIG_MODULE: &str = "@piton/config";

/// The anchor implemented by a project's configuration.
pub const CONFIG_ANCHOR: &str = "PitonConfig";

/// The source of `@piton/config`.
pub const CONFIG_SOURCE: &str = r#"// Project configuration anchors, bundled with the compiler.
//
// A project is configured by implementing `piton-config` in piton.config.pi.

export abstract anchor PitonConfig as piton-config:
    // The source root. Absolute imports such as `/a/b` resolve from here.
    root:: string

export abstract anchor FrameworkConfig as framework-config:
    // Marker anchor. Frameworks extend this with their own settings.
"#;

/// The builtin modules every build starts with.
pub fn modules() -> Vec<VirtualModule> {
    vec![VirtualModule { name: CONFIG_MODULE.to_string(), source: CONFIG_SOURCE.to_string() }]
}
