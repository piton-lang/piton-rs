//! The Piton compiler: lowering, resolution, evaluation, and the framework
//! plugin interface.

pub mod builtin;
pub mod compile;
pub mod db;
pub mod diag;
pub mod eval;
pub mod framework;
pub mod hir;
pub mod lower;
pub mod project;
pub mod resolve;
pub mod serialize;
pub mod types;
pub mod validate;
pub mod value;

/// Identifies one source file in a [`Db`](db::Db).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub u32);
