import { Fragment, useState, type ReactNode } from "react";
import { api } from "../api";

// Deliberately small renderer: fenced code blocks (with copy), inline code,
// **bold**, and plain text with preserved line breaks. Builds React nodes
// only — model output is never injected as HTML.

export default function Markdown({ text }: { text: string }) {
  const parts: ReactNode[] = [];
  const fence = /```([^\n`]*)\n([\s\S]*?)(?:```|$)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let k = 0;
  while ((m = fence.exec(text))) {
    if (m.index > last) parts.push(<Prose key={k++} text={text.slice(last, m.index)} />);
    parts.push(<CodeBlock key={k++} lang={m[1].trim()} code={m[2].replace(/\n$/, "")} />);
    last = fence.lastIndex;
  }
  if (last < text.length) parts.push(<Prose key={k++} text={text.slice(last)} />);
  return <div className="md">{parts}</div>;
}

function Prose({ text }: { text: string }) {
  const trimmed = text.replace(/^\n+|\n+$/g, "");
  if (!trimmed) return null;
  return <p>{inline(trimmed)}</p>;
}

function inline(s: string): ReactNode[] {
  const out: ReactNode[] = [];
  const re = /(`[^`\n]+`|\*\*[^*\n]+\*\*)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let k = 0;
  while ((m = re.exec(s))) {
    if (m.index > last) out.push(<Fragment key={k++}>{s.slice(last, m.index)}</Fragment>);
    const tok = m[0];
    out.push(tok.startsWith("`") ? <code key={k++}>{tok.slice(1, -1)}</code> : <strong key={k++}>{tok.slice(2, -2)}</strong>);
    last = re.lastIndex;
  }
  if (last < s.length) out.push(<Fragment key={k++}>{s.slice(last)}</Fragment>);
  return out;
}

function CodeBlock({ lang, code }: { lang: string; code: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="codeblock">
      <div className="codeblock-head">
        <span>{lang || "code"}</span>
        <button
          onClick={async () => {
            await api.copy(code);
            setCopied(true);
            window.setTimeout(() => setCopied(false), 1200);
          }}
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre>
        <code>{code}</code>
      </pre>
    </div>
  );
}
