//! The command line tool. The implementation lives in the library so that the
//! binary and the tests can share it.

#[cfg(feature = "gui")]
pub mod gui;
pub mod report;
