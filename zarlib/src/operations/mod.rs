//! High-level archive operations used by applications and the `zarc` CLI.
//! The CLI owns argument parsing and terminal UI; all archival logic
//! is implemented here.

pub mod add;
pub mod check;
pub mod common;
pub mod create;
pub mod extract;
pub mod filters;
pub mod info;
pub mod join;
pub mod list;
pub mod pipeline;
pub mod remove;

pub use add::*;
pub use check::*;
pub use common::*;
pub use create::*;
pub use extract::*;
pub use filters::*;
pub use info::*;
pub use join::*;
pub use list::*;
pub use pipeline::*;
pub use remove::*;
