//! C ABI for the SwiftUI panel. Non-macOS builds compile an empty stub so
//! `cargo test --workspace` still configures.

#[cfg(target_os = "macos")]
mod agent;
#[cfg(target_os = "macos")]
mod apps_mac;
#[cfg(target_os = "macos")]
mod dispatch;
#[cfg(target_os = "macos")]
mod exec;
#[cfg(target_os = "macos")]
mod extensions_mac;
#[cfg(target_os = "macos")]
mod files_mac;
#[cfg(target_os = "macos")]
mod host;
#[cfg(target_os = "macos")]
mod hotkeys;
#[cfg(target_os = "macos")]
mod hyperkey;
#[cfg(target_os = "macos")]
mod refresh_mac;
#[cfg(target_os = "macos")]
mod shortcuts_mac;
// ponytail: allow(dead_code) until every helper here is called. Drop the allow then.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod platform;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod window_mgmt;

#[cfg(target_os = "macos")]
pub use dispatch::dispatch;

#[cfg(target_os = "macos")]
use std::os::raw::c_char;

#[cfg(target_os = "macos")]
#[no_mangle]
pub extern "C" fn opencast_start(cb: host::Callback) {
    host::install(cb);
}

#[cfg(target_os = "macos")]
#[no_mangle]
pub extern "C" fn opencast_call(json: *const c_char) -> *mut c_char {
    host::call_raw(json)
}

#[cfg(target_os = "macos")]
#[no_mangle]
pub extern "C" fn opencast_free(p: *mut c_char) {
    host::free_raw(p);
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
fn _stub() {}
