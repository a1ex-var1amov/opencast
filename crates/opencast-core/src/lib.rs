//! OpenCast core: everything that does not need a window.
//!
//! The Tauri shell (`src-tauri`) owns the window, hotkey and OS glue; this
//! crate owns configuration, the command registry, script commands, app
//! discovery, the calculator and the AI provider adapters, so all of it can
//! be unit-tested on any machine.

pub mod apps;
pub mod calc;
pub mod commands;
pub mod config;
pub mod engine;
pub mod layouts;
pub mod providers;
pub mod scripts;

pub use commands::{Argument, Command, CommandKind, Registry};
pub use config::Config;
pub use engine::{Builtin, Engine, LayoutOp, Plan, WindowAction};
