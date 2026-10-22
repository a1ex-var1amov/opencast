//! Window geometry: where a window command puts the window. Pure functions
//! over rectangles in top-left (AX) coordinates, so every command, gap,
//! cycle step and display move is unit-tested here; the panel only reads the
//! current frame, asks for the target, and writes it.

use serde::{Deserialize, Serialize};

use crate::config::{Config, WindowCommandConfig};
use crate::engine::WindowAction;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn contains(&self, (px, py): (f64, f64)) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    /// Same frame within `tol` points (apps round sizes to their own steps).
    pub fn near(&self, o: &Rect, tol: f64) -> bool {
        (self.x - o.x).abs() <= tol
            && (self.y - o.y).abs() <= tol
            && (self.w - o.w).abs() <= tol
            && (self.h - o.h).abs() <= tol
    }

    fn clamp_into(self, work: &Rect) -> Rect {
        let w = self.w.min(work.w);
        let h = self.h.min(work.h);
        let x = self.x.max(work.x).min(work.x + work.w - w);
        let y = self.y.max(work.y).min(work.y + work.h - h);
        Rect::new(x, y, w, h)
    }
}

/// What repeating a half command does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cycle {
    /// ½ → ⅔ → ⅓ of the screen.
    Sizes,
    /// Left on a left-half window moves it to the previous display's right half.
    Displays,
    Off,
}

impl Cycle {
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "displays" => Self::Displays,
            "off" | "none" => Self::Off,
            _ => Self::Sizes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Opts {
    /// Space between tiled windows.
    pub gap: f64,
    /// Space at the screen edges.
    pub margin: f64,
    /// Almost Maximize size (fraction of the screen).
    pub almost: f64,
    /// Make Larger / Smaller step, in points per side.
    pub step: f64,
    pub cycle: Cycle,
    /// Split column commands into rows on portrait screens.
    pub portrait: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            gap: 0.0,
            margin: 0.0,
            almost: 0.9,
            step: 30.0,
            cycle: Cycle::Sizes,
            portrait: true,
        }
    }
}

impl Opts {
    pub fn from_config(c: &Config) -> Self {
        let g = &c.general;
        Self {
            gap: g.window_gap.max(0.0),
            margin: g.screen_margin.max(0.0),
            almost: g.almost_maximize.clamp(0.5, 1.0),
            step: 30.0,
            cycle: Cycle::parse(&g.window_cycle),
            portrait: !matches!(g.window_portrait.trim(), "off" | "none" | "false"),
        }
    }
}

/// How many sizes repeating `action` walks through.
pub fn cycle_len(action: WindowAction) -> usize {
    use WindowAction::*;
    match action {
        LeftHalf | RightHalf | TopHalf | BottomHalf => 3,
        _ => 1,
    }
}

const SIZES: [f64; 3] = [0.5, 2.0 / 3.0, 1.0 / 3.0];
const THIRD: f64 = 1.0 / 3.0;

/// The target as fractions `(x, y, w, h)` of the work area for tiling commands.
pub fn frac(action: WindowAction, cycle: usize, almost: f64) -> Option<(f64, f64, f64, f64)> {
    use WindowAction::*;
    let s = SIZES[cycle % 3];
    Some(match action {
        LeftHalf => (0.0, 0.0, s, 1.0),
        RightHalf => (1.0 - s, 0.0, s, 1.0),
        TopHalf => (0.0, 0.0, 1.0, s),
        BottomHalf => (0.0, 1.0 - s, 1.0, s),
        CenterHalf => (0.25, 0.0, 0.5, 1.0),
        TopLeft => (0.0, 0.0, 0.5, 0.5),
        TopRight => (0.5, 0.0, 0.5, 0.5),
        BottomLeft => (0.0, 0.5, 0.5, 0.5),
        BottomRight => (0.5, 0.5, 0.5, 0.5),
        FirstThird => (0.0, 0.0, THIRD, 1.0),
        CenterThird => (THIRD, 0.0, THIRD, 1.0),
        LastThird => (2.0 * THIRD, 0.0, THIRD, 1.0),
        FirstTwoThirds => (0.0, 0.0, 2.0 * THIRD, 1.0),
        LastTwoThirds => (THIRD, 0.0, 2.0 * THIRD, 1.0),
        TopThird => (0.0, 0.0, 1.0, THIRD),
        MiddleThird => (0.0, THIRD, 1.0, THIRD),
        BottomThird => (0.0, 2.0 * THIRD, 1.0, THIRD),
        TopTwoThirds => (0.0, 0.0, 1.0, 2.0 * THIRD),
        BottomTwoThirds => (0.0, THIRD, 1.0, 2.0 * THIRD),
        MiddleHalf => (0.0, 0.25, 1.0, 0.5),
        TopLeftSixth => (0.0, 0.0, THIRD, 0.5),
        TopCenterSixth => (THIRD, 0.0, THIRD, 0.5),
        TopRightSixth => (2.0 * THIRD, 0.0, THIRD, 0.5),
        BottomLeftSixth => (0.0, 0.5, THIRD, 0.5),
        BottomCenterSixth => (THIRD, 0.5, THIRD, 0.5),
        BottomRightSixth => (2.0 * THIRD, 0.5, THIRD, 0.5),
        // A 2 × 3 grid (made for rotated monitors, fine anywhere).
        MiddleLeftSixth => (0.0, THIRD, 0.5, THIRD),
        MiddleRightSixth => (0.5, THIRD, 0.5, THIRD),
        Maximize => (0.0, 0.0, 1.0, 1.0),
        AlmostMaximize => ((1.0 - almost) / 2.0, (1.0 - almost) / 2.0, almost, almost),
        _ => return None,
    })
}

/// A screen taller than it is wide (a rotated monitor).
pub fn is_portrait(r: &Rect) -> bool {
    r.h > r.w * 1.05
}

/// Commands whose names carry no direction ("first", "center"), laid out
/// in columns, that turn into rows on a portrait screen (as in Rectangle):
/// First Third is the top third, Center Third the middle band. Commands
/// named for a side or corner never rotate, so the name stays true.
pub fn rotates(a: WindowAction) -> bool {
    use WindowAction::*;
    matches!(
        a,
        CenterHalf | FirstThird | CenterThird | LastThird | FirstTwoThirds | LastTwoThirds
    )
}

/// `frac`, adjusted for portrait screens: direction-free commands turn into
/// rows, and the four corner sixths sit in the corners of a 2 × 3 grid
/// (wider cells) instead of a 3 × 2 one.
pub fn frac_on(action: WindowAction, cycle: usize, o: &Opts, portrait: bool) -> Option<(f64, f64, f64, f64)> {
    use WindowAction::*;
    let f = frac(action, cycle, o.almost)?;
    if !(portrait && o.portrait) {
        return Some(f);
    }
    Some(match action {
        TopLeftSixth => (0.0, 0.0, 0.5, THIRD),
        TopRightSixth => (0.5, 0.0, 0.5, THIRD),
        BottomLeftSixth => (0.0, 2.0 * THIRD, 0.5, THIRD),
        BottomRightSixth => (0.5, 2.0 * THIRD, 0.5, THIRD),
        a if rotates(a) => (f.1, f.0, f.3, f.2),
        _ => f,
    })
}

/// A fraction of the work area, with screen margins outside and half a gap
/// on each side that touches another tile.
pub fn tile(work: &Rect, (fx, fy, fw, fh): (f64, f64, f64, f64), o: &Opts) -> Rect {
    let usable = Rect::new(
        work.x + o.margin,
        work.y + o.margin,
        (work.w - 2.0 * o.margin).max(0.0),
        (work.h - 2.0 * o.margin).max(0.0),
    );
    let mut r = Rect::new(
        usable.x + fx * usable.w,
        usable.y + fy * usable.h,
        fw * usable.w,
        fh * usable.h,
    );
    let half = o.gap / 2.0;
    let eps = 1e-6;
    if fx > eps {
        r.x += half;
        r.w -= half;
    }
    if fx + fw < 1.0 - eps {
        r.w -= half;
    }
    if fy > eps {
        r.y += half;
        r.h -= half;
    }
    if fy + fh < 1.0 - eps {
        r.h -= half;
    }
    Rect::new(r.x, r.y, r.w.max(0.0), r.h.max(0.0))
}

/// Keep a window's place when it moves to another display: a tiled window
/// stays tiled the same way, a floating one keeps its relative position and
/// is scaled down if it does not fit.
pub fn move_to_display(current: &Rect, from: &Rect, to: &Rect) -> Rect {
    let fx = (current.x - from.x) / from.w;
    let fy = (current.y - from.y) / from.h;
    let fw = current.w / from.w;
    let fh = current.h / from.h;
    // Relative frame (tiled windows keep their fractions exactly).
    let mut r = Rect::new(to.x + fx * to.w, to.y + fy * to.h, fw * to.w, fh * to.h);
    // A floating window keeps its pixel size when it fits (no surprise zoom).
    let tiled = [fx, fy, fx + fw, fy + fh]
        .iter()
        .all(|v| [0.0, THIRD, 0.5, 2.0 * THIRD, 1.0].iter().any(|t| (v - t).abs() < 0.02));
    if !tiled && current.w <= to.w && current.h <= to.h {
        let (cx, cy) = (to.x + (fx + fw / 2.0) * to.w, to.y + (fy + fh / 2.0) * to.h);
        r = Rect::new(cx - current.w / 2.0, cy - current.h / 2.0, current.w, current.h);
    }
    r.clamp_into(to)
}

/// Which screen a frame is on: the one holding its centre, else the first.
pub fn screen_of(frame: &Rect, screens: &[Rect]) -> usize {
    screens.iter().position(|s| s.contains(frame.center())).unwrap_or(0)
}

/// Resolve a `[[window_commands]]` size: `0.6` = fraction, `"800px"` = points.
fn dim(v: &crate::config::WindowDim, total: f64) -> f64 {
    match v {
        crate::config::WindowDim::Fraction(f) => f * total,
        crate::config::WindowDim::Points(s) => s
            .trim()
            .trim_end_matches("px")
            .trim()
            .parse::<f64>()
            .unwrap_or(total)
            .min(total),
    }
}

/// The frame a command wants. `action` is a window command key (`left-half`,
/// `display-2`, `custom:code-left`…). `cycle` counts repeats of the same
/// command on the same window (the panel tracks it). Returns None for
/// commands the panel handles itself (restore, undo, fullscreen).
pub fn target(
    action: &str,
    current: &Rect,
    screens: &[Rect],
    cycle: usize,
    o: &Opts,
    custom: &[WindowCommandConfig],
) -> Option<Rect> {
    if screens.is_empty() {
        return None;
    }
    let si = screen_of(current, screens);
    let work = &screens[si];
    let nth = |delta: isize| -> usize {
        let n = screens.len() as isize;
        (((si as isize + delta) % n + n) % n) as usize
    };

    if let Some(id) = action.strip_prefix("custom:") {
        let c = custom.iter().find(|c| c.id == id)?;
        let target_screen = match c.display.as_deref().unwrap_or("current").trim() {
            "current" | "" => si,
            "main" => 0,
            "next" => nth(1),
            "previous" => nth(-1),
            n => n
                .parse::<usize>()
                .ok()
                .map(|n| n.saturating_sub(1).min(screens.len() - 1))
                .unwrap_or(si),
        };
        let wa = &screens[target_screen];
        let usable = tile(wa, (0.0, 0.0, 1.0, 1.0), &Opts { gap: 0.0, ..*o });
        let w = dim(&c.w, usable.w);
        let h = dim(&c.h, usable.h);
        let x = usable.x + dim(&c.x, usable.w);
        let y = usable.y + dim(&c.y, usable.h);
        return Some(Rect::new(x, y, w, h).clamp_into(wa));
    }
    if let Some(n) = action.strip_prefix("display-") {
        let i = n.parse::<usize>().ok()?.checked_sub(1)?;
        let to = screens.get(i)?;
        return Some(move_to_display(current, work, to));
    }
    let a = WindowAction::from_key(action)?;
    use WindowAction::*;
    match a {
        NextDisplay => Some(move_to_display(current, work, &screens[nth(1)])),
        PreviousDisplay => Some(move_to_display(current, work, &screens[nth(-1)])),
        // Displays mode: repeating Left/Right half continues onto the neighbouring display.
        LeftHalf | RightHalf if o.cycle == Cycle::Displays && cycle > 0 && screens.len() > 1 => {
            let (to, other) = if a == LeftHalf {
                (nth(-1), RightHalf)
            } else {
                (nth(1), LeftHalf)
            };
            Some(tile(&screens[to], frac(other, 0, o.almost)?, o))
        }
        Center => {
            let usable = tile(work, (0.0, 0.0, 1.0, 1.0), &Opts { gap: 0.0, ..*o });
            let w = current.w.min(usable.w);
            let h = current.h.min(usable.h);
            Some(Rect::new(
                usable.x + (usable.w - w) / 2.0,
                usable.y + (usable.h - h) / 2.0,
                w,
                h,
            ))
        }
        ReasonableSize => {
            let usable = tile(work, (0.0, 0.0, 1.0, 1.0), o);
            let (w, h) = (usable.w * 0.6, usable.h * 0.7);
            Some(Rect::new(
                usable.x + (usable.w - w) / 2.0,
                usable.y + (usable.h - h) / 2.0,
                w,
                h,
            ))
        }
        MaximizeHeight => {
            let usable = tile(work, (0.0, 0.0, 1.0, 1.0), o);
            Some(Rect::new(current.x, usable.y, current.w, usable.h).clamp_into(work))
        }
        MaximizeWidth => {
            let usable = tile(work, (0.0, 0.0, 1.0, 1.0), o);
            Some(Rect::new(usable.x, current.y, usable.w, current.h).clamp_into(work))
        }
        MakeLarger | MakeSmaller => {
            let d = if a == MakeLarger { o.step } else { -o.step };
            let (cx, cy) = current.center();
            let w = (current.w + 2.0 * d).clamp(200.0, work.w);
            let h = (current.h + 2.0 * d).clamp(150.0, work.h);
            Some(Rect::new(cx - w / 2.0, cy - h / 2.0, w, h).clamp_into(work))
        }
        MoveLeft | MoveRight | MoveUp | MoveDown => {
            let usable = tile(work, (0.0, 0.0, 1.0, 1.0), o);
            let mut r = Rect::new(current.x, current.y, current.w.min(usable.w), current.h.min(usable.h));
            match a {
                MoveLeft => r.x = usable.x,
                MoveRight => r.x = usable.x + usable.w - r.w,
                MoveUp => r.y = usable.y,
                _ => r.y = usable.y + usable.h - r.h,
            }
            Some(r)
        }
        RestoreSize | Undo | ToggleFullscreen => None,
        _ => {
            let steps = if o.cycle == Cycle::Sizes { cycle } else { 0 };
            Some(tile(work, frac_on(a, steps, o, is_portrait(work))?, o))
        }
    }
}

/// The region a command fills on a unit screen (for its icon), or None.
/// With `portrait`, the unit screen stands for a rotated monitor: the region
/// is still in 0…1 fractions, but rotating commands come out as rows.
pub fn glyph(action: &str, custom: &[WindowCommandConfig], portrait: bool) -> Option<Rect> {
    // A tall unit-area screen so `is_portrait` sees it; scaled back to 0…1 below.
    let (sw, sh) = if portrait { (1.0, 1.6) } else { (1.0, 1.0) };
    let screen = Rect::new(0.0, 0.0, sw, sh);
    let floating = Rect::new(0.3 * sw, 0.3 * sh, 0.4 * sw, 0.4 * sh);
    let o = Opts {
        step: 0.08,
        ..Opts::default()
    };
    match action {
        "next-display" | "previous-display" | "restore" | "undo" | "fullscreen" => None,
        a if a.starts_with("display-") => None,
        a => target(a, &floating, &[screen], 0, &o, custom).map(|r| {
            // Make Larger/Smaller etc. on the unit screen: keep inside, no min sizes.
            Rect::new(r.x / sw, r.y / sh, (r.w / sw).min(1.0), (r.h / sh).min(1.0))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WindowDim;

    const W: Rect = Rect {
        x: 0.0,
        y: 25.0,
        w: 1200.0,
        h: 800.0,
    };

    fn t(a: &str, cur: Rect, cycle: usize, o: &Opts) -> Rect {
        target(a, &cur, &[W], cycle, o, &[]).unwrap()
    }

    #[test]
    fn halves_cycle_through_sizes() {
        let o = Opts::default();
        let cur = Rect::new(100.0, 100.0, 300.0, 300.0);
        assert!(t("left-half", cur, 0, &o).near(&Rect::new(0.0, 25.0, 600.0, 800.0), 0.01));
        assert!(t("left-half", cur, 1, &o).near(&Rect::new(0.0, 25.0, 800.0, 800.0), 0.01));
        assert!(t("left-half", cur, 2, &o).near(&Rect::new(0.0, 25.0, 400.0, 800.0), 0.01));
        assert!(t("left-half", cur, 3, &o).near(&Rect::new(0.0, 25.0, 600.0, 800.0), 0.01));
        assert!(t("right-half", cur, 1, &o).near(&Rect::new(400.0, 25.0, 800.0, 800.0), 0.01));
        let off = Opts { cycle: Cycle::Off, ..o };
        assert!(t("left-half", cur, 1, &off).near(&Rect::new(0.0, 25.0, 600.0, 800.0), 0.01));
    }

    #[test]
    fn gaps_and_margins() {
        let o = Opts {
            gap: 10.0,
            margin: 20.0,
            ..Opts::default()
        };
        let cur = Rect::new(100.0, 100.0, 300.0, 300.0);
        let left = t("left-half", cur, 0, &o);
        let right = t("right-half", cur, 0, &o);
        assert!(left.near(&Rect::new(20.0, 45.0, 575.0, 760.0), 0.01), "{left:?}");
        assert!(right.near(&Rect::new(605.0, 45.0, 575.0, 760.0), 0.01), "{right:?}");
        assert!((right.x - (left.x + left.w) - 10.0).abs() < 0.01, "gap between tiles");
        let max = t("maximize", cur, 0, &o);
        assert!(max.near(&Rect::new(20.0, 45.0, 1160.0, 760.0), 0.01));
    }

    #[test]
    fn every_tiling_command_fits_the_screen() {
        let o = Opts {
            gap: 8.0,
            margin: 8.0,
            ..Opts::default()
        };
        let cur = Rect::new(100.0, 100.0, 300.0, 300.0);
        for (a, key, ..) in crate::engine::WINDOW_ACTIONS {
            if let Some(r) = target(key, &cur, &[W], 0, &o, &[]) {
                assert!(r.w > 0.0 && r.h > 0.0, "{key}");
                assert!(r.x >= W.x - 0.01 && r.x + r.w <= W.x + W.w + 0.01, "{key} {r:?}");
                assert!(r.y >= W.y - 0.01 && r.y + r.h <= W.y + W.h + 0.01, "{key} {r:?}");
            } else {
                assert!(
                    matches!(
                        a,
                        WindowAction::RestoreSize | WindowAction::Undo | WindowAction::ToggleFullscreen
                    ),
                    "{key}"
                );
            }
        }
    }

    #[test]
    fn sizes_and_moves() {
        let o = Opts::default();
        let cur = Rect::new(100.0, 100.0, 400.0, 300.0);
        let c = t("center", cur, 0, &o);
        assert!(c.near(&Rect::new(400.0, 275.0, 400.0, 300.0), 0.01));
        let bigger = t("make-larger", cur, 0, &o);
        assert!(bigger.near(&Rect::new(70.0, 70.0, 460.0, 360.0), 0.01));
        let smaller = t("make-smaller", Rect::new(100.0, 100.0, 220.0, 160.0), 0, &o);
        assert!(smaller.w >= 200.0 && smaller.h >= 150.0);
        assert_eq!(t("move-right", cur, 0, &o).x, 800.0);
        assert_eq!(t("move-down", cur, 0, &o).y, 525.0);
        assert_eq!(t("maximize-height", cur, 0, &o).h, 800.0);
        let r = t("reasonable-size", cur, 0, &o);
        assert!((r.w - 720.0).abs() < 0.01 && (r.h - 560.0).abs() < 0.01);
        assert!(target("restore", &cur, &[W], 0, &o, &[]).is_none());
    }

    #[test]
    fn display_moves_keep_the_layout() {
        let laptop = Rect::new(0.0, 25.0, 1440.0, 875.0);
        let external = Rect::new(1440.0, 0.0, 2560.0, 1415.0);
        let o = Opts::default();
        // A left-half window stays a left half.
        let left = Rect::new(0.0, 25.0, 720.0, 875.0);
        let moved = target("next-display", &left, &[laptop, external], 0, &o, &[]).unwrap();
        assert!(moved.near(&Rect::new(1440.0, 0.0, 1280.0, 1415.0), 0.5), "{moved:?}");
        // A small floating window keeps its size, centred at the same relative place.
        let floating = Rect::new(500.0, 300.0, 400.0, 300.0);
        let moved = target("next-display", &floating, &[laptop, external], 0, &o, &[]).unwrap();
        assert_eq!((moved.w, moved.h), (400.0, 300.0));
        assert!(external.contains(moved.center()));
        // Wraps around; display-N is absolute.
        let back = target("next-display", &moved, &[laptop, external], 0, &o, &[]).unwrap();
        assert!(laptop.contains(back.center()));
        let d1 = target("display-1", &moved, &[laptop, external], 0, &o, &[]).unwrap();
        assert!(laptop.contains(d1.center()));
        assert!(target("display-3", &moved, &[laptop, external], 0, &o, &[]).is_none());
        // Displays cycle mode: Left again on a left-half window → previous display's right half.
        let dm = Opts {
            cycle: Cycle::Displays,
            ..o
        };
        let on_ext = Rect::new(1440.0, 0.0, 1280.0, 1415.0);
        let r = target("left-half", &on_ext, &[laptop, external], 1, &dm, &[]).unwrap();
        assert!(r.near(&Rect::new(720.0, 25.0, 720.0, 875.0), 0.5), "{r:?}");
    }

    #[test]
    fn custom_commands_and_glyphs() {
        let custom = vec![WindowCommandConfig {
            id: "code".into(),
            title: "Code".into(),
            x: WindowDim::Fraction(0.0),
            y: WindowDim::Fraction(0.0),
            w: WindowDim::Fraction(0.6),
            h: WindowDim::Points("500px".into()),
            display: None,
            hotkey: None,
            icon: None,
        }];
        let r = target(
            "custom:code",
            &Rect::new(10.0, 40.0, 100.0, 100.0),
            &[W],
            0,
            &Opts::default(),
            &custom,
        )
        .unwrap();
        assert!(r.near(&Rect::new(0.0, 25.0, 720.0, 500.0), 0.01), "{r:?}");
        let g = glyph("left-half", &[], false).unwrap();
        assert!(g.near(&Rect::new(0.0, 0.0, 0.5, 1.0), 1e-9));
        assert!(glyph("next-display", &[], false).is_none());
        assert!(glyph("custom:code", &custom, false).is_some());
    }

    #[test]
    fn portrait_screens_split_into_rows() {
        let tall = Rect::new(0.0, 0.0, 1080.0, 1920.0);
        let cur = Rect::new(100.0, 100.0, 500.0, 500.0);
        let o = Opts::default();
        let run = |a: &str, o: &Opts| target(a, &cur, &[tall], 0, o, &[]).unwrap();
        // Center Third is the middle band on a rotated monitor.
        assert!(run("center-third", &o).near(&Rect::new(0.0, 640.0, 1080.0, 640.0), 1e-6));
        assert!(run("first-two-thirds", &o).near(&Rect::new(0.0, 0.0, 1080.0, 1280.0), 1e-6));
        assert!(run("center-half", &o).near(&Rect::new(0.0, 480.0, 1080.0, 960.0), 1e-6));
        // Halves keep their meaning.
        assert!(run("left-half", &o).near(&Rect::new(0.0, 0.0, 540.0, 1920.0), 1e-6));
        // Explicit row commands work everywhere, and `window_portrait = "off"` keeps columns.
        assert!(run("middle-third", &o).near(&Rect::new(0.0, 640.0, 1080.0, 640.0), 1e-6));
        let off = Opts { portrait: false, ..o };
        assert!(run("center-third", &off).near(&Rect::new(360.0, 0.0, 360.0, 1920.0), 1e-6));
        // Landscape screens are untouched.
        let wide = target("center-third", &cur, &[Rect::new(0.0, 0.0, 1920.0, 1080.0)], 0, &o, &[]).unwrap();
        assert!(wide.near(&Rect::new(640.0, 0.0, 640.0, 1080.0), 1e-6));
        // Icons follow the screen.
        let g = glyph("center-third", &[], true).unwrap();
        assert!(g.near(&Rect::new(0.0, THIRD, 1.0, THIRD), 1e-9));
    }

    /// Every command named for a side, corner or band lands there, on wide and tall screens.
    #[test]
    fn names_match_where_windows_go() {
        let o = Opts::default();
        let e = 1e-6;
        for (w, h) in [(1920.0, 1080.0), (1080.0, 1920.0)] {
            let s = Rect::new(0.0, 0.0, w, h);
            let cur = Rect::new(w * 0.3, h * 0.3, w * 0.2, h * 0.2);
            for (_, key, title, _) in crate::engine::WINDOW_ACTIONS {
                let Some(r) = target(key, &cur, &[s], 0, &o, &[]) else {
                    continue;
                };
                let t = title.to_lowercase();
                let words: Vec<&str> = t.split(' ').collect();
                let has = |x: &str| words.contains(&x);
                if ["move", "maximize", "make", "almost", "reasonable", "center"].contains(&words[0])
                    && words.len() <= 2
                    && !has("half")
                    && !has("third")
                {
                    continue;
                }
                let ctx = format!("{title} on {w}x{h}: {r:?}");
                if has("left") {
                    assert!(r.x.abs() < e, "{ctx}");
                }
                if has("right") {
                    assert!((r.x + r.w - w).abs() < e, "{ctx}");
                }
                if has("top") {
                    assert!(r.y.abs() < e, "{ctx}");
                }
                if has("bottom") {
                    assert!((r.y + r.h - h).abs() < e, "{ctx}");
                }
                if has("middle") {
                    assert!(r.y > e && r.y + r.h < h - e, "{ctx}");
                }
                if has("center") && !has("half") && !has("third") || has("center") && w > h {
                    assert!(r.x > e && r.x + r.w < w - e, "{ctx}");
                }
                if has("left") || has("right") {
                    assert!(r.w < w - e, "{ctx}");
                }
                if has("top") || has("bottom") {
                    assert!(r.h < h - e, "{ctx}");
                }
            }
        }
    }
}
