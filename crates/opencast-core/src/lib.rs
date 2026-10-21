//! OpenCast core: everything that does not need a window.
//!
//! The macOS shell (`crates/opencast-ffi` and `macos/`) owns the window, hotkey
//! and OS glue; this crate owns configuration, the command registry, script
//! commands, app discovery, the calculator and the AI provider adapters, so
//! all of it can be unit-tested on any machine.

pub mod apps;
pub mod calc;
pub mod commands;
pub mod config;
pub mod emoji;
pub mod engine;
pub mod files;
pub mod layouts;
pub mod notes;
pub mod pins;
pub mod providers;
pub mod scripts;
pub mod usage;

pub use commands::{Argument, Command, CommandKind, Registry};
pub use config::Config;
pub use engine::{Builtin, Engine, LayoutOp, Plan, WindowAction};
