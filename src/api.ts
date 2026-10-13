// Typed bridge to the Rust core. In a plain browser (`npm run dev` without
// Tauri) every call falls back to an in-memory mock so the UI can be worked on
// without the native shell.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { mock } from "./mock";

export type CommandKind =
  | "builtin"
  | "app"
  | "script"
  | "ai"
  | "quicklink"
  | "calculator"
  | "window"
  | "layout";

export interface Argument {
  placeholder: string;
  optional: boolean;
  secure: boolean;
}

export interface Command {
  id: string;
  title: string;
  subtitle?: string | null;
  icon?: string | null;
  kind: CommandKind;
  arguments?: Argument[];
}

export interface ChatInit {
  prompt?: string | null;
  system?: string | null;
  provider?: string | null;
  model?: string | null;
  autoSend: boolean;
}

export type ExecResult =
  | { type: "hide" }
  | { type: "toast"; text: string }
  | { type: "output"; title: string; text: string }
  | ({ type: "chat" } & ChatInit)
  | { type: "error"; message: string }
  | { type: "args"; command: Command };

export interface ProviderInfo {
  id: string;
  kind: "openai" | "anthropic" | "gemini";
  models: string[];
  defaultModel: string;
  isDefault: boolean;
}

export type Role = "system" | "user" | "assistant";

export interface Message {
  role: Role;
  content: string;
}

export interface StreamArgs {
  requestId: string;
  provider?: string | null;
  model?: string | null;
  system?: string | null;
  messages: Message[];
}

export type ChatEvent =
  | { type: "delta"; text: string }
  | { type: "done" }
  | { type: "error"; text: string };

export interface AiEvent {
  requestId: string;
  event: ChatEvent;
}

type Unlisten = () => void;

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const api = {
  search: (query: string): Promise<Command[]> =>
    isTauri ? invoke("search", { query }) : mock.search(query),

  execute: (id: string, args: string[] = []): Promise<ExecResult> =>
    isTauri ? invoke("execute", { id, args }) : mock.execute(id, args),

  warnings: (): Promise<string[]> => (isTauri ? invoke("warnings") : Promise.resolve([])),

  providers: (): Promise<ProviderInfo[]> =>
    isTauri ? invoke("list_providers") : mock.providers(),

  stream: (args: StreamArgs): Promise<void> =>
    isTauri ? invoke("ai_stream", { args }) : mock.stream(args),

  cancel: (requestId: string): Promise<void> =>
    isTauri ? invoke("ai_cancel", { requestId }) : mock.cancel(requestId),

  hide: (): Promise<void> => (isTauri ? invoke("hide_window") : Promise.resolve()),

  copy: (text: string): Promise<void> =>
    isTauri ? invoke("copy_text", { text }) : navigator.clipboard?.writeText(text) ?? Promise.resolve(),

  paste: (text: string): Promise<void> =>
    isTauri ? invoke("paste_text", { text }) : Promise.resolve(),

  onAiEvent: (cb: (e: AiEvent) => void): Promise<Unlisten> =>
    isTauri ? listen<AiEvent>("ai://event", (e) => cb(e.payload)) : mock.onAiEvent(cb),

  /** Results of commands fired from global hotkeys. */
  onExecResult: (cb: (r: ExecResult) => void): Promise<Unlisten> =>
    isTauri ? listen<ExecResult>("launcher://result", (e) => cb(e.payload)) : Promise.resolve(() => {}),

  onShown: (cb: () => void): Promise<Unlisten> =>
    isTauri ? listen("launcher://shown", () => cb()) : Promise.resolve(() => {}),
};

export function newId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
