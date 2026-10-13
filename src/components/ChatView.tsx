import { useCallback, useEffect, useRef, useState } from "react";
import { api, newId, type ChatInit, type ProviderInfo } from "../api";
import type { Flash } from "../App";
import Footer from "./Footer";
import Markdown from "./Markdown";

interface Turn {
  role: "user" | "assistant";
  content: string;
  error?: string;
}

interface Props {
  init: ChatInit;
  onBack: () => void;
  flash: Flash;
}

export default function ChatView({ init, onBack, flash }: Props) {
  const [providers, setProviders] = useState<ProviderInfo[] | null>(null);
  const [provider, setProvider] = useState<string | undefined>(init.provider ?? undefined);
  const [model, setModel] = useState<string | undefined>(init.model ?? undefined);
  const [turns, setTurns] = useState<Turn[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const streamId = useRef<string | null>(null);
  const turnsRef = useRef<Turn[]>([]);
  const started = useRef(false);
  const box = useRef<HTMLTextAreaElement>(null);
  const scroller = useRef<HTMLDivElement>(null);

  turnsRef.current = turns;

  useEffect(() => {
    api.providers().then((ps) => {
      setProviders(ps);
      setProvider((p) => p ?? (ps.find((x) => x.isDefault) ?? ps[0])?.id);
    });
  }, []);

  const info = providers?.find((p) => p.id === provider);
  const activeModel = model ?? info?.defaultModel;

  // Stream events for the request we started.
  useEffect(() => {
    const un = api.onAiEvent(({ requestId, event }) => {
      if (requestId !== streamId.current) return;
      if (event.type === "delta") {
        setTurns((t) => t.map((x, i) => (i === t.length - 1 ? { ...x, content: x.content + event.text } : x)));
        return;
      }
      if (event.type === "error") {
        setTurns((t) => t.map((x, i) => (i === t.length - 1 ? { ...x, error: event.text } : x)));
      }
      streamId.current = null;
      setStreaming(false);
      box.current?.focus();
    });
    return () => void un.then((f) => f());
  }, []);

  useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [turns]);

  const send = useCallback(
    (text: string) => {
      const content = text.trim();
      if (!content || streamId.current) return;
      // Failed or empty answers are not sent back as history.
      const history = turnsRef.current.filter((t) => !(t.role === "assistant" && (t.error || !t.content)));
      const next: Turn[] = [...history, { role: "user", content }];
      const id = newId();
      streamId.current = id;
      setStreaming(true);
      setTurns([...next, { role: "assistant", content: "" }]);
      setInput("");
      api
        .stream({
          requestId: id,
          provider,
          model,
          system: init.system,
          messages: next.map(({ role, content }) => ({ role, content })),
        })
        .catch((e) => {
          setTurns((t) => t.map((x, i) => (i === t.length - 1 ? { ...x, error: String(e) } : x)));
          streamId.current = null;
          setStreaming(false);
        });
    },
    [provider, model, init.system],
  );

  // First render: send the AI command's prompt, or prefill the box.
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    if (init.prompt && init.autoSend) send(init.prompt);
    else if (init.prompt) setInput(init.prompt);
    box.current?.focus();
  }, [init, send]);

  const stop = () => {
    if (streamId.current) void api.cancel(streamId.current);
    streamId.current = null;
    setStreaming(false);
  };

  const lastAnswer = [...turns].reverse().find((t) => t.role === "assistant" && t.content)?.content;

  const onKeyDown = (e: React.KeyboardEvent) => {
    const mod = e.metaKey || e.ctrlKey;
    if (e.key === "Escape") {
      e.preventDefault();
      if (streaming) stop();
      else onBack();
    } else if (e.key === "Enter" && mod) {
      e.preventDefault();
      if (lastAnswer) void api.paste(lastAnswer);
    } else if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      send(input);
    } else if (mod && e.shiftKey && e.key.toLowerCase() === "c") {
      e.preventDefault();
      if (lastAnswer) api.copy(lastAnswer).then(() => flash("Copied answer"));
    } else if (mod && e.key.toLowerCase() === "n") {
      e.preventDefault();
      stop();
      setTurns([]);
    }
  };

  const noProviders = providers !== null && providers.length === 0;

  return (
    <div className="view chat" onKeyDown={onKeyDown}>
      <div className="searchbar chat-bar">
        <button className="back" onClick={onBack} aria-label="Back">
          ←
        </button>
        <span className="args-title">AI Chat</span>
        <div className="pickers">
          <select
            aria-label="Provider"
            value={provider ?? ""}
            onChange={(e) => {
              setProvider(e.target.value);
              setModel(undefined);
            }}
            disabled={!providers?.length}
          >
            {providers?.map((p) => (
              <option key={p.id} value={p.id}>
                {p.id}
              </option>
            ))}
          </select>
          <select aria-label="Model" value={activeModel ?? ""} onChange={(e) => setModel(e.target.value)} disabled={!info}>
            {info?.models.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="messages" ref={scroller}>
        {noProviders && (
          <div className="empty">
            No AI provider configured. Run <strong>Open Config Folder</strong>, add a <code>[[providers]]</code> entry to
            config.toml, then <strong>Reload Commands</strong>.
          </div>
        )}
        {!noProviders && turns.length === 0 && (
          <div className="empty">
            Ask anything. Answers stream from <strong>{provider ?? "your default provider"}</strong>
            {activeModel ? ` · ${activeModel}` : ""}.
          </div>
        )}
        {turns.map((t, i) => (
          <div key={i} className={`msg msg-${t.role}`}>
            {t.role === "user" ? (
              <div className="bubble">{t.content}</div>
            ) : (
              <>
                {t.content ? <Markdown text={t.content} /> : !t.error && <span className="typing" aria-label="Thinking" />}
                {t.error && <div className="msg-error">{t.error}</div>}
              </>
            )}
          </div>
        ))}
      </div>

      <div className="composer">
        <textarea
          ref={box}
          rows={1}
          value={input}
          placeholder={streaming ? "Streaming… Esc to stop" : "Message — Enter to send, Shift+Enter for a new line"}
          onChange={(e) => setInput(e.target.value)}
          spellCheck
        />
      </div>

      <Footer
        hints={[
          ["Paste answer", "⌘↵"],
          ["Copy", "⌘⇧C"],
          ["New", "⌘N"],
          [streaming ? "Stop" : "Back", "esc"],
        ]}
        left={streaming ? <span className="brand">Streaming…</span> : undefined}
      />
    </div>
  );
}
