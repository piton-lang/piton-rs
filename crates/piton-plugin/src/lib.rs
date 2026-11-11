//! Plugins: commands and behaviour added to Piton in TypeScript, run in
//! QuickJS.
//!
//! A plugin is declared in Piton, with a `piton-plugin` anchor that names its
//! commands, their arguments and what it may do on the machine, and written in
//! TypeScript, which Piton transpiles and runs itself. See
//! `spec/scope/plugins` for the whole design.

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
