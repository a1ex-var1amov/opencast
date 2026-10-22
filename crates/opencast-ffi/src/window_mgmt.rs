//! Window geometry, the Seek (Click Text) OCR scan and synthetic clicks.
//!
//! Moving other apps' windows and capturing layouts happens in the Swift
//! shell through the Accessibility API (`macos/WindowManager.swift`); the
//! engine only decides *what* to do (`WindowAction`) and this module keeps the
//! pure geometry so it stays testable off-macOS.

use anyhow::{anyhow, bail, Result};
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

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use anyhow::Context;
    use serde::Deserialize;
    use std::time::Duration;

    async fn osascript(script: &str, args: &[String]) -> Result<String> {
        let mut cmd = tokio::process::Command::new("osascript");
        cmd.args(["-l", "JavaScript", "-e", script])
            .args(args)
            .kill_on_drop(true);
        let out = tokio::time::timeout(Duration::from_secs(20), cmd.output())
            .await
            .map_err(|_| anyhow!("timed out running the text recognizer"))?
            .context("running osascript")?;
        if !out.status.success() {
            bail!(
                "text recognition error: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn parse<T: serde::de::DeserializeOwned>(out: &str) -> Result<T> {
        serde_json::from_str(out).with_context(|| format!("unexpected reply: {out}"))
    }

    /// The monitor (as a logical rect) containing `point`, falling back to
    /// the first monitor.
    fn monitor_at(x: f64, y: f64) -> Result<Rect> {
        use core_graphics::display::CGDisplay;
        let ids = CGDisplay::active_displays().map_err(|e| anyhow!("no monitors found ({e})"))?;
        let rects: Vec<Rect> = ids
            .into_iter()
            .map(|id| {
                let bounds = CGDisplay::new(id).bounds();
                Rect {
                    x: bounds.origin.x,
                    y: bounds.origin.y,
                    w: bounds.size.width,
                    h: bounds.size.height,
                }
            })
            .collect();
        monitor_containing(&rects, x, y).ok_or_else(|| anyhow!("no monitors found"))
    }

    const OCR: &str = r#"
ObjC.import('Vision'); ObjC.import('Foundation');
function run(argv) {
  const url = $.NSURL.fileURLWithPath(argv[0]);
  const req = $.VNRecognizeTextRequest.alloc.init;
  req.recognitionLevel = $.VNRequestTextRecognitionLevelAccurate;
  const handler = $.VNImageRequestHandler.alloc.initWithURLOptions(url, $.NSDictionary.dictionary);
  if (!handler.performRequestsError($.NSArray.arrayWithObject(req), null)) return JSON.stringify({ error: 'text recognition failed' });
  const res = req.results, lines = [];
  for (let i = 0; i < res.count; i++) {
    const o = res.objectAtIndex(i), cand = o.topCandidates(1);
    if (!cand.count) continue;
    const c = cand.objectAtIndex(0), s = ObjC.unwrap(c.string), b = o.boundingBox;   // normalized, origin bottom-left
    const words = []; const re = /\S+/g; let m;
    while ((m = re.exec(s)) !== null) {
      try {
        const wb = c.boundingBoxForRangeError($.NSMakeRange(m.index, m[0].length), null).boundingBox;
        words.push({ at: m.index, len: m[0].length, x: wb.origin.x, w: wb.size.width });
      } catch (e) {}
    }
    lines.push({ label: s, x: b.origin.x, y: 1 - b.origin.y - b.size.height, w: b.size.width, h: b.size.height, words: words });
  }
  return JSON.stringify({ lines: lines });
}
"#;

    #[derive(Deserialize)]
    struct Ocr {
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        lines: Vec<SeekHit>,
    }

    /// Full display bounds under the cursor, in Core Graphics points
    /// (origin at the top-left of the primary display, y down).
    fn cursor_monitor() -> Result<Rect> {
        use core_graphics::event::CGEvent;
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        let source =
            CGEventSource::new(CGEventSourceStateID::CombinedSessionState).map_err(|_| anyhow!("no event source"))?;
        let event = CGEvent::new(source).map_err(|_| anyhow!("no event"))?;
        let point = event.location();
        monitor_at(point.x, point.y)
    }

    /// Screenshot the display under the cursor and OCR it. Hits come back in
    /// screen points, with the monitor rect (for the overlay).
    // ponytail: one display, OCR only (no accessibility-tree search).
    pub async fn seek_scan() -> Result<SeekScan> {
        let monitor = cursor_monitor()?;
        let png = std::env::temp_dir().join(format!("opencast-seek-{}.png", std::process::id()));
        let region = [monitor.x, monitor.y, monitor.w, monitor.h]
            .map(|v| format!("{}", v.round()))
            .join(",");
        let shot = tokio::process::Command::new("screencapture")
            .args(["-x", "-R", &region])
            .arg(&png)
            .output()
            .await
            .context("running screencapture")?;
        if !shot.status.success() || !png.exists() {
            bail!(
                "OpenCast needs the Screen Recording permission to find text on screen: \
                 System Settings → Privacy & Security → Screen Recording → enable OpenCast, \
                 then try again"
            );
        }
        let reply = osascript(OCR, &[png.to_string_lossy().into_owned()]).await;
        let _ = std::fs::remove_file(&png);
        let r: Ocr = parse(&reply?)?;
        if let Some(e) = r.error {
            bail!("{e}");
        }
        if r.lines.is_empty() {
            bail!("no readable text on screen");
        }
        let hits = r
            .lines
            .into_iter()
            .map(|l| SeekHit {
                label: l.label,
                x: monitor.x + l.x * monitor.w,
                y: monitor.y + l.y * monitor.h,
                w: l.w * monitor.w,
                h: l.h * monitor.h,
                words: l
                    .words
                    .into_iter()
                    .map(|w| SeekWord {
                        at: w.at,
                        len: w.len,
                        x: monitor.x + w.x * monitor.w,
                        w: w.w * monitor.w,
                    })
                    .collect(),
            })
            .collect();
        Ok(SeekScan { monitor, hits })
    }

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

/// OCR the display under the cursor for the Seek overlay.
pub async fn seek_scan() -> Result<SeekScan> {
    mac::seek_scan().await
}

/// Click at a point in screen coordinates.
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
}
