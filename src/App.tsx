import { useCallback, useEffect, useRef, useState } from "react";
import { api, type ChatInit, type Command, type ExecResult } from "./api";
import SearchView from "./components/SearchView";
import ArgsView from "./components/ArgsView";
import OutputView from "./components/OutputView";
import ChatView from "./components/ChatView";

type View =
  | { name: "search" }
  | { name: "args"; command: Command }
  | { name: "output"; title: string; text: string }
  | { name: "chat"; init: ChatInit; key: number };

export type Flash = (text: string, opts?: { error?: boolean; thenHide?: boolean }) => void;

export default function App() {
  const [view, setView] = useState<View>({ name: "search" });
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<{ text: string; error: boolean } | null>(null);
  const toastTimer = useRef<number | undefined>(undefined);

  const flash: Flash = useCallback((text, opts = {}) => {
    window.clearTimeout(toastTimer.current);
    setToast({ text, error: !!opts.error });
    toastTimer.current = window.setTimeout(
      () => {
        setToast(null);
        if (opts.thenHide) void api.hide();
      },
      opts.error ? 4000 : 1300,
    );
  }, []);

  const toSearch = useCallback((clear = false) => {
    if (clear) setQuery("");
    setView({ name: "search" });
  }, []);

  // Each time the hotkey shows the window: drop transient views, focus input.
  useEffect(() => {
    const un = api.onShown(() => {
      setView((v) => (v.name === "output" || v.name === "args" ? { name: "search" } : v));
      window.dispatchEvent(new Event("opencast:focus"));
    });
    return () => void un.then((f) => f());
  }, []);

  // Config warnings (bad TOML, missing keys, non-executable scripts) once at start.
  useEffect(() => {
    api.warnings().then((w) => w.length && flash(w[0] + (w.length > 1 ? ` (+${w.length - 1} more)` : ""), { error: true }));
  }, [flash]);

  // Results of commands fired from global hotkeys (registered below, once
  // `handle` exists).

  const handle = useCallback(
    (r: ExecResult) => {
      switch (r.type) {
        case "hide":
          toSearch(true);
          void api.hide();
          break;
        case "toast":
          toSearch(true);
          flash(r.text, { thenHide: true });
          break;
        case "output":
          setView({ name: "output", title: r.title, text: r.text });
          break;
        case "chat":
          setView({ name: "chat", key: Date.now(), init: r });
          break;
        case "error":
          flash(r.message, { error: true });
          break;
        case "args":
          setView({ name: "args", command: r.command });
          break;
      }
    },
    [flash, toSearch],
  );

  useEffect(() => {
    const un = api.onExecResult(handle);
    return () => void un.then((f) => f());
  }, [handle]);

  const run = useCallback(
    async (cmd: Command, args?: string[]) => {
      if (cmd.arguments?.length && !args) {
        setView({ name: "args", command: cmd });
        return;
      }
      setBusy(true);
      try {
        handle(await api.execute(cmd.id, args ?? []));
      } catch (e) {
        flash(String(e), { error: true });
      } finally {
        setBusy(false);
      }
    },
    [handle, flash],
  );

  const ask = useCallback((q: string) => {
    setView({ name: "chat", key: Date.now(), init: { prompt: q, autoSend: true } });
  }, []);

  return (
    <div className="panel">
      {view.name === "search" && (
        <SearchView query={query} setQuery={setQuery} onRun={run} onAsk={ask} busy={busy} />
      )}
      {view.name === "args" && (
        <ArgsView command={view.command} busy={busy} onSubmit={(a) => run(view.command, a)} onBack={() => toSearch()} />
      )}
      {view.name === "output" && (
        <OutputView title={view.title} text={view.text} onBack={() => toSearch()} flash={flash} />
      )}
      {view.name === "chat" && <ChatView key={view.key} init={view.init} onBack={() => toSearch(true)} flash={flash} />}
      {toast && (
        <div className={`toast${toast.error ? " toast-error" : ""}`} role="status">
          {toast.text}
        </div>
      )}
    </div>
  );
}
