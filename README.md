# OpenCast

An open-source, keyboard-first launcher in the spirit of Raycast — with AI that
works with **any provider you choose** and no account, subscription or licence
key. MIT licensed. The panel is SwiftUI and the engine is Rust. macOS first;
the core is portable.

- **⌥Space** opens a panel: fuzzy search over apps, script commands, AI commands
  and quicklinks; type maths for an inline calculator.
- **Tab** turns whatever you typed into an AI chat. Answers stream in; **⌘↵**
  pastes the answer into the app you came from.
- **AI commands** ("Fix Spelling and Grammar", "Explain This"…) run a prompt on
  the text selected in the frontmost app.
- **Any AI provider**: three wire adapters — OpenAI-compatible, Anthropic,
  Gemini — cover OpenAI, Claude, Gemini, Ollama, LM Studio, OpenRouter, Groq,
  vLLM, LiteLLM and anything else that speaks one of them. Mix them: run
  grammar fixes on a local model and chat on a cloud one.
- **Script commands** in any language, using Raycast's header format
  (`@raycast.*` headers are accepted), so existing scripts mostly work as-is.
- **Window management** (Rectangle-style): Left/Right/Top/Bottom Half,
  Quarters, Thirds, Maximize, Center — searchable in the launcher or bound to
  hotkeys.
- **Layouts**: *Remember Layout* saves where every window is; *Restore
  Layout: \<name\>* puts them back, launching missing apps.
- **Global hotkeys**: bind any command — an app, a window action, an AI
  command — to a key.
- **Hyper key** (Superkey-style): `hyperkey = true` makes Caps Lock send
  ⌃⌥⌘⇧ while held and open the launcher when tapped; `Hyper+X` works in
  `[[hotkeys]]`.
- **Click Text** (Superkey's Seek): type what you see in the front window and
  OpenCast clicks it, via on-device OCR. Needs the Screen Recording permission.
- **Private by default**: no telemetry; the only network traffic is to the AI
  endpoints in your config. Keys stay in the Keychain (or any secret manager).

## Quick start

Requirements: Rust (stable) and the Xcode command-line tools.

```bash
cargo test                 # core tests: providers, scripts, search, calculator
./build-app.command        # build and install ~/Applications/OpenCast.app
```

On first launch OpenCast writes `~/.config/opencast/config.toml` and a few
example scripts, then shows the panel. The default config points at a local
**Ollama** (`http://localhost:11434/v1`), so with `ollama pull llama3.2` AI
works without any key.

### macOS permissions

AI commands that read your **selection**, **Paste answer**, **window
management** and **layouts** all drive other apps through macOS accessibility,
which needs **System Settings → Privacy & Security → Accessibility →
OpenCast**. Everything else works without it.

## Windows and layouts

Type "half", "maximize", "third"… in the launcher to snap the window you were
just using (Rectangle-style). **Remember Layout** saves every visible window's
position under a name; **Restore Layout: \<name\>** puts everything back and
launches apps that aren't running. Multi-monitor placement is restored as
absolute coordinates, so a layout saved on your desk works best on your desk.

Bind anything to a global hotkey in `config.toml`:

```toml
[[hotkeys]]
key = "Ctrl+Alt+Left"
command = "Left Half"        # window action

[[hotkeys]]
key = "Cmd+Alt+S"
command = "Slack"            # focus-or-launch an app

[[hotkeys]]
key = "Cmd+Alt+G"
command = "ai:fix-grammar"   # run an AI command on the selection
```

No window hotkeys ship enabled, so OpenCast won't fight Rectangle if you still
run it.

`hyperkey = true` in `[general]` turns Caps Lock into a hyper key (restart
OpenCast to apply): held, it adds ⌃⌥⌘⇧ to whatever you press, so `Hyper+K`
shortcuts work in any app and in `[[hotkeys]]`; tapped alone, it opens the
launcher. OpenCast remaps Caps Lock with `hidutil` on launch and undoes it on
*Quit OpenCast*. The bundled *Use Caps Lock as Launcher Key* script is the
hyper-less version: Caps Lock → F18, then `hotkey = "F18"`.

*Click Text* is Superkey's Seek: it reads the screen with macOS's on-device
OCR and overlays a search pill — type what you see, every match is
highlighted, `;` or Tab cycles, Enter clicks, Esc cancels. Bind it to
`Hyper+C`. The first run asks for the Screen Recording permission. If you
build OpenCast yourself, macOS drops that one permission after every rebuild
(it is tied to the exact binary): toggle OpenCast off and on under Screen
Recording and relaunch.

## Configuring providers

```toml
[general]
hotkey = "Alt+Space"
default_provider = "claude"

[[providers]]
id = "claude"
kind = "anthropic"
api_key_cmd = "security find-generic-password -s opencast-anthropic -w"
model = "claude-sonnet-5-5"
models = ["claude-opus-5-5", "claude-haiku-5-5"]

[[providers]]
id = "ollama"
kind = "openai"
base_url = "http://localhost:11434/v1"
model = "llama3.2"

[[ai_commands]]
id = "fix-grammar"
title = "Fix Spelling and Grammar"
provider = "ollama"            # optional; defaults to default_provider
system = "Return only the corrected text."
prompt = "Fix spelling and grammar:\n\n{selection}"
```

| `kind` | Use for | Notes |
| --- | --- | --- |
| `openai` | OpenAI, Ollama, LM Studio, OpenRouter, Groq, Together, vLLM, LiteLLM | set `base_url`; key optional for local servers |
| `anthropic` | Claude models | `max_tokens` defaults to 4096 |
| `gemini` | Gemini models | uses `streamGenerateContent?alt=sse` |

**Keys** never go in the TOML. Use `api_key_env = "VAR"` or
`api_key_cmd = "<command that prints the key>"`. GUI apps on macOS don't see
your shell's environment, so `api_key_cmd` with the Keychain is the reliable
choice:

```bash
security add-generic-password -s opencast-anthropic -a "$USER" -w 'sk-ant-...'
```

`api_key_cmd` also works with 1Password (`op read op://vault/item/key`), `pass`,
etc. Extra headers (e.g. OpenRouter's `HTTP-Referer`) go in `headers = { ... }`.

Prompt placeholders: `{selection}` (selected text in the frontmost app) and
`{input}` (asked for after you pick the command).

After editing, run **Reload Commands** from the launcher.

## Script commands

Drop an executable into `~/.config/opencast/scripts/`:

```bash
#!/bin/bash
# @opencast.title Toggle Dark Mode
# @opencast.mode silent            # silent | compact | fullOutput
# @opencast.icon 🌗
# @opencast.argument1 { "type": "text", "placeholder": "name", "optional": true }
osascript -e 'tell app "System Events" to tell appearance preferences to set dark mode to not dark mode'
```

`silent` closes the panel, `compact` shows the last line of output as a toast,
`fullOutput` shows everything. Up to three arguments; `"type": "password"`
masks input. Scripts run out of process with a 30 s timeout
(`general.script_timeout_secs`). `chmod +x` is required.

## Keys

| Where | Key | Does |
| --- | --- | --- |
| Search | ↑ ↓ / ⌃N ⌃P | move |
| | ↵ | run / open / copy calculator result |
| | ⇥ | ask AI about the typed text |
| | esc | clear, then close |
| Chat | ↵ / ⇧↵ | send / new line |
| | ⌘↵ | paste last answer into the previous app |
| | ⌘⇧C | copy last answer |
| | ⌘N | new chat |
| | esc | stop streaming, then back |

## Layout

```
crates/opencast-core/   portable logic: config, registry + fuzzy search,
  src/providers/          scripts, app discovery, calculator, AI adapters (+ tests)
crates/opencast-ffi/    macOS C ABI: hotkeys, clipboard, paste, window management
macos/                  SwiftUI panel
examples/               default config.toml and example scripts
```

The panel never holds keys or talks to the network; it calls the Rust engine
and receives streamed tokens from it.

## Status and roadmap

v0.1 is an MVP. Next (v0.2): Keychain-backed key entry in a settings UI, native
app icons, clipboard history and snippets, language-agnostic JSON plugins over
stdio, tool calling. Later (v1): MCP servers as commands, Linux/Windows
packaging (including window management via `wmctrl`/KWin), signed releases, a
Homebrew cask, and Superkey-style "seek & click" (keyboard-click any visible
text via accessibility/OCR).

Prior art worth knowing: [SuperCmd](https://github.com/SuperCmdLabs/SuperCmd)
(Electron, Raycast-extension compatible) and
[Vicinae](https://docs.vicinae.com) (Linux, Qt).

## License

MIT
