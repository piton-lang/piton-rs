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

    // Optional, and so not declared here: `entry`, the file or directory a
    // build starts from; `frameworks`, one configuration anchor per framework;
    // and `libraries`, a name for each directory outside the root that absolute
    // imports may also reach:
    //
    //     libraries:
    //         customLib: ../lib
    //
    // which makes `/customLib/Thing` the `Thing` in that directory. The root is
    // searched first, so a library can never change an import that resolved.
    //
    // Also optional: `sharedRoot`, one directory outside the root that shared
    // imports resolve against:
    //
    //     sharedRoot: ../shared
    //
    // which makes `//Thing` the `Thing` in that directory. Where `libraries`
    // names several and spells the name into every path, this names the one
    // place several projects share, and `//` is the whole of the prefix.

export abstract anchor FrameworkConfig as framework-config:
    // Marker anchor. Frameworks extend this with their own settings.
"#;

/// The builtin modules every build starts with.
pub fn modules() -> Vec<VirtualModule> {
    vec![VirtualModule { name: CONFIG_MODULE.to_string(), source: CONFIG_SOURCE.to_string() }]
}
