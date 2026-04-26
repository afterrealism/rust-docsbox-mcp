//! Tool implementations. Each module owns one tool family and is wired
//! into the MCP `tool_router` from `crate::server`.

pub mod clippy;
pub mod crates_io;
pub mod docs;
pub mod playground;
pub mod run_locally;
pub mod rustc_explain;
pub mod rustfmt;

pub(crate) mod util;
