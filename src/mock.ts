// Browser-only stand-in for the Rust core, used when the UI runs outside
// Tauri (e.g. `npm run dev` in a normal browser). Not shipped behaviour.
import type { AiEvent, Command, ExecResult, ProviderInfo, StreamArgs } from "./api";

const commands: Command[] = [
  { id: "builtin:chat", title: "AI Chat", icon: "💬", kind: "builtin" },
  { id: "ai:fix-grammar", title: "Fix Spelling and Grammar", subtitle: "AI · ollama", icon: "✍️", kind: "ai" },
  { id: "ai:explain", title: "Explain This", subtitle: "AI · ollama", icon: "💡", kind: "ai" },
  {
    id: "ai:shell",
    title: "Shell Command For…",
    subtitle: "AI · ollama",
    icon: "⌨️",
    kind: "ai",
    arguments: [{ placeholder: "Input", optional: false, secure: false }],
  },
  {
    id: "script:uuid",
    title: "Generate UUID",
    subtitle: "Script",
    icon: "🆔",
    kind: "script",
    arguments: [{ placeholder: "lowercase? (y/n)", optional: true, secure: false }],
  },
  { id: "script:ip", title: "Network Addresses", subtitle: "Local and public IP addresses", icon: "🌐", kind: "script" },
  {
    id: "link:search-github",
    title: "Search GitHub",
    subtitle: "github.com",
    icon: "🐙",
    kind: "quicklink",
    arguments: [{ placeholder: "Query", optional: false, secure: false }],
  },
  { id: "builtin:reload", title: "Reload Commands", icon: "🔄", kind: "builtin" },
  { id: "window:left-half", title: "Left Half", subtitle: "Move the front window", icon: "◧", kind: "window" },
  { id: "window:maximize", title: "Maximize", subtitle: "Move the front window", icon: "⛶", kind: "window" },
  {
    id: "layout:remember",
    title: "Remember Layout",
    subtitle: "Save where every window is right now",
    icon: "📸",
    kind: "layout",
    arguments: [{ placeholder: "Layout name", optional: false, secure: false }],
  },
  { id: "layout:restore:work", title: "Restore Layout: Work", subtitle: "5 windows", icon: "🗂", kind: "layout" },
  ...["Safari", "Slack", "System Settings", "Terminal", "Notes", "Visual Studio Code", "Finder", "Calendar"].map(
    (name): Command => ({ id: `app:${name}`, title: name, subtitle: "Application", kind: "app" }),
  ),
];

function score(title: string, q: string): number {
  const t = title.toLowerCase();
  if (t.startsWith(q)) return 3;
  if (t.split(/\s+/).some((w) => w.startsWith(q))) return 2;
  let i = 0;
  for (const c of t) if (c === q[i]) i++;
  return i === q.length ? 1 : 0;
}

let listeners: ((e: AiEvent) => void)[] = [];
const timers = new Map<string, number[]>();

export const mock = {
  async search(query: string): Promise<Command[]> {
    const q = query.trim().toLowerCase();
    const out: Command[] = [];
    if (/^[\d\s.+\-*/()^%]+$/.test(q) && /\d/.test(q) && /[+\-*/^%]/.test(q)) {
      try {
        const v = Function(`"use strict"; return (${q.replace(/\^/g, "**")})`)();
        if (Number.isFinite(v)) {
          out.push({ id: `calc:${v}`, title: String(+v.toFixed(10)), subtitle: `= ${query.trim()} · Enter to copy`, icon: "🧮", kind: "calculator" });
        }
      } catch {
        /* not maths */
      }
    }
    if (!q) return [...out, ...commands.filter((c) => c.kind !== "app"), ...commands.filter((c) => c.kind === "app")];
    return [
      ...out,
      ...commands
        .map((c) => [score(c.title, q), c] as const)
        .filter(([s]) => s > 0)
        .sort((a, b) => b[0] - a[0])
        .map(([, c]) => c),
    ];
  },

  async execute(id: string, args: string[]): Promise<ExecResult> {
    if (id.startsWith("calc:")) return { type: "toast", text: `Copied ${id.slice(5)}` };
    if (id === "builtin:chat") return { type: "chat", autoSend: false };
    if (id === "ai:shell") return { type: "chat", prompt: args[0], system: "Reply with one shell command.", autoSend: true };
    if (id === "ai:fix-grammar") return { type: "error", message: "No text selected. Select text in another app first." };
    if (id === "script:uuid") return { type: "toast", text: "Copied 3F2504E0-4F89-11D3-9A0C-0305E82C3301" };
    if (id === "script:ip") return { type: "output", title: "Network Addresses", text: "Local:\n  en0  192.168.1.24\n\nPublic:\n  203.0.113.7" };
    if (id === "builtin:reload") return { type: "toast", text: `Reloaded · ${commands.length} commands` };
    if (id.startsWith("window:")) return { type: "hide" };
    if (id === "layout:remember") return { type: "toast", text: `Remembered “${args[0]}” · 7 windows` };
    if (id.startsWith("layout:restore:")) return { type: "toast", text: "Restored “Work”" };
    return { type: "hide" };
  },

  async providers(): Promise<ProviderInfo[]> {
    return [
      { id: "ollama", kind: "openai", models: ["llama3.2", "qwen2.5-coder"], defaultModel: "llama3.2", isDefault: true },
      { id: "claude", kind: "anthropic", models: ["claude-sonnet-5-5", "claude-haiku-5-5"], defaultModel: "claude-sonnet-5-5", isDefault: false },
    ];
  },

  async stream(args: StreamArgs): Promise<void> {
    const last = args.messages[args.messages.length - 1]?.content ?? "";
    const reply =
      `Here's a mock answer from **${args.provider ?? "ollama"}** to: "${last.slice(0, 60)}"\n\n` +
      "```bash\nfind . -type f -size +100M -exec ls -lh {} \\;\n```\n\nThis lists every file over 100 MB with its size. " +
      "Run the real app with `npm run tauri dev` to talk to an actual model.";
    const words = reply.split(/(?<=\s)/);
    const emit = (e: AiEvent) => listeners.forEach((l) => l(e));
    const ids = words.map((w, i) => window.setTimeout(() => emit({ requestId: args.requestId, event: { type: "delta", text: w } }), 25 * i));
    ids.push(window.setTimeout(() => emit({ requestId: args.requestId, event: { type: "done" } }), 25 * words.length + 10));
    timers.set(args.requestId, ids);
  },

  async cancel(requestId: string): Promise<void> {
    timers.get(requestId)?.forEach((t) => window.clearTimeout(t));
    timers.delete(requestId);
  },

  async onAiEvent(cb: (e: AiEvent) => void): Promise<() => void> {
    listeners.push(cb);
    return () => {
      listeners = listeners.filter((l) => l !== cb);
    };
  },
};
