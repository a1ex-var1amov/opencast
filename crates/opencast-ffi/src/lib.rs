//! C ABI for the SwiftUI panel. Non-macOS builds compile an empty stub so
//! `cargo test --workspace` still configures.

#[cfg(target_os = "macos")]
mod dispatch;

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
fn _stub() {}
