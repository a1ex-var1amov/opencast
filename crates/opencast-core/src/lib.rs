//! OpenCast core: everything that does not need a window.
//!
//! The macOS shell (`crates/opencast-ffi` and `macos/`) owns the window, hotkey
//! and OS glue; this crate owns configuration, the command registry, script
//! commands, app discovery, the calculator and the AI provider adapters, so
//! all of it can be unit-tested on any machine.

pub mod alfred;
pub mod apps;
pub mod bookmarks;
pub mod calc;
pub mod chats;
pub mod commands;
pub mod config;
pub mod currency;
pub mod doctor;
pub mod emoji;
mod emoji_data;
pub mod encode;
pub mod engine;
pub mod extensions;
pub mod files;
pub mod history;
pub mod keymap;
pub mod layouts;
pub mod mcp;
pub mod notes;
pub mod panes;
pub mod pins;
pub mod pipeline;
pub mod providers;
pub mod recents;
pub mod scripts;
pub mod spotify;
pub mod sync;
pub mod tz;
pub mod updates;
pub mod usage;
pub mod webtext;
pub mod window;

pub use commands::{Argument, Command, CommandKind, Registry};
pub use config::Config;
pub use engine::{Builtin, Engine, LayoutOp, Plan, WindowAction};
