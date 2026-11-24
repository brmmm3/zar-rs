//! High-level archive operations used by applications and the `zarc` CLI.
//! The CLI owns argument parsing and terminal UI; all archival logic
//! is implemented here.

pub mod add;
mod archive_input;
pub mod check;
pub mod common;
pub mod create;
pub mod extract;
mod extraction_checksum;
pub mod filters;
pub mod grep;
pub mod info;
pub mod join;
pub mod list;
pub mod meta;
pub mod pipeline;
pub mod remove;
pub mod remove_commits;
mod stream;
pub mod transform;
pub mod undo;
mod worker_priority;

#[cfg(test)]
mod coverage_tests;

pub use add::*;
pub use archive_input::is_supported_import_archive;
pub use check::*;
pub use common::*;
pub use create::*;
pub use extract::*;
pub use extraction_checksum::extraction_needs_checksum_recheck;
pub use filters::*;
pub use grep::*;
pub use info::*;
pub use join::*;
pub use list::*;
pub use meta::*;
pub use pipeline::*;
pub use remove::*;
pub use remove_commits::*;
pub use stream::*;
pub use transform::*;
pub use undo::*;
