//! Window geometry and synthetic Seek clicks.
//!
//! Moving other apps' windows and capturing layouts happens in the Swift
//! shell through the Accessibility API (`macos/WindowManager.swift`); the
//! engine only decides *what* to do (`WindowAction`) and this module keeps the
//! pure geometry so it stays testable off-macOS. Seek OCR is in-process Vision
//! in `macos/SeekView.swift` (`SeekOCR`); the `SeekHit` types here document the
//! JSON shape the panel still accepts from a `seek` event.

use anyhow::{anyhow, Result};
use opencast_core::engine::WindowAction;
use serde::{Deserialize, Serialize};

/// Logical (point) rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub fn monitor_containing(rects: &[Rect], x: f64, y: f64) -> Option<Rect> {
    rects
        .iter()
        .find(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h)
        .or(rects.first())
        .copied()
}

#[cfg(test)]
mod monitor_tests {
    use super::*;

    #[test]
    fn picks_the_display_under_the_point_else_the_first() {
        let rects = [
            Rect {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: 800.0,
            },
            Rect {
                x: 1000.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            },
        ];
        assert_eq!(monitor_containing(&rects, 1200.0, 10.0).unwrap().x, 1000.0);
        assert_eq!(monitor_containing(&rects, -5.0, -5.0).unwrap().x, 0.0);
        assert!(monitor_containing(&[], 0.0, 0.0).is_none());
    }
}

/// One OCR line, in screen points, with per-word x extents so the overlay
/// can box the matched characters exactly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeekHit {
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default)]
    pub words: Vec<SeekWord>,
}

/// A word inside a line: char offset/length in `label`, x extent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeekWord {
    pub at: usize,
    pub len: usize,
    pub x: f64,
    pub w: f64,
}

/// Everything the Seek overlay needs: the monitor to cover and the lines.
/// Kept for the wire shape / tests; live scans are built in Swift.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct SeekScan {
    pub monitor: Rect,
    pub hits: Vec<SeekHit>,
}

/// Compute the target rect for an action within a work area. Pure, so it is
/// testable off-macOS. `current` is used by `Center` (which keeps size).
pub fn target_rect(action: WindowAction, work: Rect, current: Rect) -> Rect {
    match action.frac() {
        Some((fx, fy, fw, fh)) => Rect {
            x: work.x + fx * work.w,
            y: work.y + fy * work.h,
            w: fw * work.w,
            h: fh * work.h,
        },
        None => Rect {
            x: work.x + (work.w - current.w).max(0.0) / 2.0,
            y: work.y + (work.h - current.h).max(0.0) / 2.0,
            w: current.w,
            h: current.h,
        },
    }
}

// ponytail: Seek OCR lives in macos/SeekView.swift (SeekOCR); Rust only clicks.
#[cfg(target_os = "macos")]
mod mac {
    use super::*;

    pub fn click_at(x: f64, y: f64) -> Result<()> {
        use core_graphics::event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        use core_graphics::geometry::CGPoint;
        let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| anyhow!("no event source"))?;
        let pt = CGPoint::new(x, y);
        for kind in [
            CGEventType::MouseMoved,
            CGEventType::LeftMouseDown,
            CGEventType::LeftMouseUp,
        ] {
            CGEvent::new_mouse_event(src.clone(), kind, pt, CGMouseButton::Left)
                .map_err(|_| anyhow!("could not synthesize a click"))?
                .post(CGEventTapLocation::HID);
        }
        Ok(())
    }
}

/// Click at a point in screen coordinates.
#[cfg(target_os = "macos")]
pub fn click_at(x: f64, y: f64) -> Result<()> {
    mac::click_at(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect {
        x: 0.0,
        y: 25.0,
        w: 1512.0,
        h: 957.0,
    };

    #[test]
    fn halves_and_quarters() {
        let cur = Rect {
            x: 100.0,
            y: 100.0,
            w: 600.0,
            h: 400.0,
        };
        let left = target_rect(WindowAction::LeftHalf, WORK, cur);
        assert_eq!(
            left,
            Rect {
                x: 0.0,
                y: 25.0,
                w: 756.0,
                h: 957.0
            }
        );
        let br = target_rect(WindowAction::BottomRight, WORK, cur);
        assert_eq!(br.x, 756.0);
        assert_eq!(br.y, 25.0 + 478.5);
        let center = target_rect(WindowAction::Center, WORK, cur);
        assert_eq!(center.w, 600.0);
        assert_eq!(center.x, (1512.0 - 600.0) / 2.0);
        let max = target_rect(WindowAction::Maximize, WORK, cur);
        assert_eq!((max.x, max.y, max.w, max.h), (0.0, 25.0, 1512.0, 957.0));
    }

    #[test]
    fn oversized_window_centers_at_origin() {
        let cur = Rect {
            x: 0.0,
            y: 0.0,
            w: 2000.0,
            h: 1200.0,
        };
        let c = target_rect(WindowAction::Center, WORK, cur);
        assert_eq!((c.x, c.y), (WORK.x, WORK.y));
    }

    #[test]
    fn seek_hit_json_keeps_word_boxes() {
        let hit: SeekHit = serde_json::from_str(
            r#"{"label":"Hi there","x":10,"y":20,"w":100,"h":12,"words":[{"at":0,"len":2,"x":10,"w":30}]}"#,
        )
        .unwrap();
        assert_eq!(hit.label, "Hi there");
        assert_eq!(hit.words.len(), 1);
        assert_eq!(hit.words[0].at, 0);
        assert_eq!(hit.words[0].w, 30.0);
        let bare: SeekHit = serde_json::from_str(r#"{"label":"x","x":0,"y":0,"w":1,"h":1}"#).unwrap();
        assert!(bare.words.is_empty());
    }
}
