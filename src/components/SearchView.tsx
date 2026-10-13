import { useEffect, useMemo, useRef, useState } from "react";
import { api, type Command, type CommandKind } from "../api";
import Footer from "./Footer";

type Row = { type: "cmd"; cmd: Command } | { type: "ask" };

const KIND_LABEL: Record<CommandKind, string> = {
  app: "Application",
  ai: "AI Command",
  script: "Script",
  quicklink: "Quicklink",
  builtin: "Command",
  calculator: "Calculator",
  window: "Window",
  layout: "Layout",
};

interface Props {
  query: string;
  setQuery: (q: string) => void;
  onRun: (c: Command) => void;
  onAsk: (q: string) => void;
  busy: boolean;
}

export default function SearchView({ query, setQuery, onRun, onAsk, busy }: Props) {
  const [results, setResults] = useState<Command[]>([]);
  const [sel, setSel] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const seq = useRef(0);

  useEffect(() => {
    const id = ++seq.current;
    api
      .search(query)
      .then((r) => {
        if (id === seq.current) {
          setResults(r);
          setSel(0);
        }
      })
      .catch(() => {});
  }, [query]);

  useEffect(() => {
    const focus = () => {
      input.current?.focus();
      input.current?.select();
    };
    focus();
    window.addEventListener("opencast:focus", focus);
    return () => window.removeEventListener("opencast:focus", focus);
  }, []);

  const rows: Row[] = useMemo(() => {
    const r: Row[] = results.map((cmd) => ({ type: "cmd", cmd }));
    if (query.trim()) r.push({ type: "ask" });
    return r;
  }, [results, query]);

  useEffect(() => {
    list.current?.querySelector(`[data-index="${sel}"]`)?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const activate = (i: number) => {
    const row = rows[i];
    if (!row) return;
    if (row.type === "ask") onAsk(query.trim());
    else onRun(row.cmd);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const down = e.key === "ArrowDown" || (e.ctrlKey && e.key === "n");
    const up = e.key === "ArrowUp" || (e.ctrlKey && e.key === "p");
    if (down || up) {
      e.preventDefault();
      setSel((s) => (rows.length ? (s + (down ? 1 : -1) + rows.length) % rows.length : 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      activate(sel);
    } else if (e.key === "Tab") {
      e.preventDefault();
      if (query.trim()) onAsk(query.trim());
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (query) setQuery("");
      else void api.hide();
    }
  };

  const current = rows[sel];
  const action =
    current?.type === "ask"
      ? "Ask AI"
      : current?.cmd.kind === "calculator"
        ? "Copy"
        : current?.cmd.arguments?.length
          ? "Continue"
          : current?.cmd.kind === "app"
            ? "Open"
            : "Run";

  return (
    <>
      <div className="searchbar">
        <input
          ref={input}
          className="search-input"
          value={query}
          placeholder="Search apps and commands, or ask AI…"
          spellCheck={false}
          autoComplete="off"
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          aria-label="Search"
        />
        {busy && <span className="spinner" aria-label="Working" />}
      </div>
      <div className="list" ref={list} role="listbox">
        {rows.length === 0 && <div className="empty">No results</div>}
        {rows.map((row, i) => (
          <div
            key={row.type === "ask" ? "ask" : row.cmd.id}
            data-index={i}
            role="option"
            aria-selected={i === sel}
            className={`row${i === sel ? " selected" : ""}${row.type === "ask" ? " row-ask" : ""}`}
            onMouseMove={() => i !== sel && setSel(i)}
            onClick={() => activate(i)}
          >
            {row.type === "ask" ? (
              <>
                <span className="icon icon-ask">✦</span>
                <span className="title">
                  Ask AI <span className="quote">“{query.trim()}”</span>
                </span>
                <span className="kind">
                  <kbd>Tab</kbd>
                </span>
              </>
            ) : (
              <>
                <Icon cmd={row.cmd} />
                <span className="title">{row.cmd.title}</span>
                {row.cmd.subtitle && <span className="subtitle">{row.cmd.subtitle}</span>}
                <span className="kind">{KIND_LABEL[row.cmd.kind]}</span>
              </>
            )}
          </div>
        ))}
      </div>
      <Footer
        hints={
          current?.type === "ask"
            ? [[action, "↵"]]
            : [
                [action, "↵"],
                ["Ask AI", "⇥"],
              ]
        }
      />
    </>
  );
}

function Icon({ cmd }: { cmd: Command }) {
  if (cmd.icon) return <span className="icon">{cmd.icon}</span>;
  // Native app icons arrive in v0.2; until then a stable coloured monogram.
  let h = 0;
  for (const c of cmd.title) h = (h * 31 + c.charCodeAt(0)) % 360;
  return (
    <span className="icon icon-mono" style={{ background: `hsl(${h} 55% 48%)` }}>
      {cmd.title.charAt(0).toUpperCase()}
    </span>
  );
}
