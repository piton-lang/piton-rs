//! Plugins add commands and behaviour to Piton. A plugin uses TypeScript, and
//! QuickJS runs it.
//!
//! Piton source declares a plugin with a `piton-plugin` anchor. The anchor
//! names the commands of the plugin and their arguments. It also tells what
//! the plugin can do on the machine. The code of the plugin is TypeScript.
//! Piton transpiles and runs this code itself. See `spec/scope/plugins` for
//! the full design.

pub mod bundled;
pub mod host;
pub mod inspection;
pub mod manifest;
pub mod output;
pub mod plan;
pub mod reach;
pub mod space;
pub mod transpile;

pub use host::{run, run_build, run_command, run_inspect, run_reach, Failure, Invocation, Services, Stream};
pub use manifest::{builders, discover, load_order, unclaimed, ArgKind, ArgSpec, Capabilities, CommandSpec, Manifest, Problem, Source};
