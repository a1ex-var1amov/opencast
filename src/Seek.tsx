import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "./api";

type Rect = { x: number; y: number; w: number; h: number };
type Word = { at: number; len: number; x: number; w: number };
type Hit = Rect & { label: string; words?: Word[] };

/** Screen x of character index `i` in a line: exact when Vision gave word
 * boxes, else an even-glyph-width estimate across the line. */
function charX(h: Hit, i: number): number {
  const words = h.words ?? [];
  const w = words.filter((w) => w.at <= i).pop();
  if (w) return w.x + (Math.min(i - w.at, w.len) / w.len) * w.w;
  return h.x + (h.w / Math.max(h.label.length, 1)) * i;
}
type Scan = { monitor: Rect; hits: Hit[] };

/**
 * Superkey-style "Seek & click". The shell OCRs the display under the cursor
 * and sends every line (in screen points) with the monitor this overlay
 * covers. Typing filters; each occurrence is boxed; Tab or ";" cycles (as in
 * Superkey); Enter clicks; Esc closes.
 */
export default function Seek() {
  const [scan, setScan] = useState<Scan | null>(null);
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!isTauri) {
      setScan(DEMO);
      return;
    }
    const un = listen<Scan>("seek://scan", (e) => {
      setScan(e.payload);
      setQuery("");
      setIndex(0);
      input.current?.focus();
    });
    return () => void un.then((f) => f());
  }, []);

  const matches = useMemo<Rect[]>(() => {
    const needle = query.trim().toLowerCase();
    if (!scan || !needle) return [];
    const out: Rect[] = [];
    for (const h of scan.hits) {
      const text = h.label.toLowerCase();
      for (let at = text.indexOf(needle); at !== -1; at = text.indexOf(needle, at + 1)) {
        const x0 = charX(h, at);
        const x1 = charX(h, at + needle.length);
        out.push({ x: x0 - scan.monitor.x, y: h.y - scan.monitor.y, w: Math.max(x1 - x0, 4), h: h.h });
      }
    }
    return out;
  }, [scan, query]);

  const n = matches.length;
  const cur = n ? ((index % n) + n) % n : 0;
  const current = n ? matches[cur] : null;

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      void invoke("seek_close");
    } else if (e.key === "Tab" || e.key === ";" || e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      setIndex((i) => i + (e.shiftKey || e.key === "ArrowUp" ? -1 : 1));
    } else if (e.key === "Enter" && current && scan) {
      e.preventDefault();
      void invoke("seek_click", {
        x: scan.monitor.x + current.x + current.w / 2,
        y: scan.monitor.y + current.y + current.h / 2,
      });
    }
  };

  const pill = { x: window.innerWidth / 2, y: 48 }; // bottom centre of the island

  return (
    <div className="seek" onKeyDown={onKeyDown}>
      {matches.map((m, i) => (
        <div
          key={i}
          className={"seek-hit" + (i === cur ? " seek-current" : "")}
          style={{ left: m.x - 3, top: m.y - 2, width: m.w + 6, height: m.h + 4 }}
        />
      ))}
      {current && (
        <svg className="seek-line">
          <line x1={pill.x} y1={pill.y} x2={current.x + current.w / 2} y2={current.y - 2} />
        </svg>
      )}
      <div className="seek-pill">
        <span className="seek-icon">◎</span>
        <input
          ref={input}
          autoFocus
          value={query}
          placeholder="Type what you see…"
          spellCheck={false}
          onChange={(e) => {
            setQuery(e.target.value);
            setIndex(0);
          }}
        />
        <span className="seek-count">{n ? `${cur + 1} of ${n} · ; next` : query.trim() ? "0 of 0" : ""}</span>
      </div>
    </div>
  );
}

/** Browser-only stand-in so `npm run dev` + /seek.html shows something. */
const DEMO: Scan = {
  monitor: { x: 0, y: 0, w: 1440, h: 900 },
  hits: [
    { label: "Ryan Hanson", x: 120, y: 160, w: 220, h: 30, words: [{ at: 0, len: 4, x: 120, w: 90 }, { at: 5, len: 6, x: 220, w: 120 }] },
    { label: "Hi! I'm Ryan and I like to make", x: 520, y: 320, w: 420, h: 20 },
    { label: "elegant optimizations for macOS.", x: 520, y: 356, w: 440, h: 20 },
  ],
};
