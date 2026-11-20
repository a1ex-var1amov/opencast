//! Poisoned locks keep their data. One panic must not stick the launcher on "internal error".

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

static LOGGED: AtomicBool = AtomicBool::new(false);

fn note_poison() {
    // ponytail: one log line per process. Per-lock logs if a single lock is the one that keeps dying.
    if !LOGGED.swap(true, Ordering::Relaxed) {
        tracing::error!("a lock was poisoned; later calls keep the data that was inside");
    }
}

pub fn mutex<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| {
        note_poison();
        e.into_inner()
    })
}

pub fn read<T>(m: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    m.read().unwrap_or_else(|e| {
        note_poison();
        e.into_inner()
    })
}

pub fn write<T>(m: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    m.write().unwrap_or_else(|e| {
        note_poison();
        e.into_inner()
    })
}
