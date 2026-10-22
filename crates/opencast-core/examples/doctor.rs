//! Health report for a config folder (default: ~/.config/opencast).
//!
//!     cargo run --release -p opencast-core --example doctor [-- /path/to/config]
//!
//! Exits 1 when something is broken, so scripts and CI can use it.

use std::time::Instant;

use opencast_core::doctor::{self, Level, Report};
use opencast_core::{apps, Config, Engine};

fn main() {
    let dir = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(Config::config_dir);
    println!("OpenCast Doctor · {}\n", dir.display());
    // Files first: a runaway config must not be loaded at all.
    let mut pre = Report::default();
    doctor::check_files(&dir, &mut pre);
    if pre.worst() == Level::Error {
        print!("{}", pre.to_text());
        std::process::exit(1);
    }
    let t = Instant::now();
    let engine = Engine::load_with_apps(&dir, apps::scan());
    let report = doctor::run(&engine, Some(t.elapsed()));
    print!("{}", report.to_text());
    if report.worst() == Level::Error {
        std::process::exit(1);
    }
}
