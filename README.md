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
  `@opencast.mode list` makes a script an Alfred-style **Script Filter**.
- **Calculator** with functions, percentages, hex/binary and unit conversion
  (`15% of 80`, `sqrt 2`, `0xff + 1`, `10 km in mi`, `72 f to c`).
- **Customizable**: aliases, icons, boosts and hiding for any command
  (`[[commands]]`), ordered fallbacks, a `[keys]` keymap, a theme file, and
  per-command AI options (`temperature`, `output = "replace"`).
- **Window management** (Rectangle-style): halves, quarters, thirds, two
  thirds, sixths, maximize, almost maximize, move between displays, grow and
  shrink, undo — searchable or bound to hotkeys (Rectangle's own key set is one
  line away: `window_hotkeys = "rectangle"`). Repeating a half cycles its
  size, a highlighted command outlines where the window will go, hotkeys show a
  small HUD, *Arrange Window* is a picker grid, and `drag_snap` adds
  drag-to-edge snapping. Your own positions go in `[[window_commands]]`.
  **Spaces**: *Switch Space* lists Mission Control's desktops with what runs on
  each, and moving a window to one takes you there too.
- **Layouts**: *Remember Layout* saves where every window is; *Restore
  Layout: \<name\>* puts them back, launching missing apps.
- **Global hotkeys**: bind any command — an app, a window action, an AI
  command — to a key.
- **Hyper key** (Superkey-style): `hyperkey = true` makes Caps Lock send
  ⌃⌥⌘⇧ while held and open the launcher when tapped; `Hyper+X` works in
  `[[hotkeys]]`.
- **Click Text** (Superkey's Seek): type what you see in the front window and
  OpenCast clicks it, via on-device OCR. Needs the Screen Recording permission.
- **MCP tools**: add `[[mcp_servers]]` and every server's tools become
  launcher commands *and* tools the AI chat can call (with your approval) —
  thousands of integrations, no extension store needed. Servers that advertise
  resources or prompt templates get those as commands too.
- **Shortcuts.app**: every shortcut you have is a command.
- **Learns what you pick**: type `sl`, choose Slack once, and Slack is the top
  hit for `sl` from then on. Recent use counts more than old use; `vsc` finds
  Visual Studio Code.
- **Actions and preview**: ⌘K shows actions for the selected row (Open With,
  Show in Finder, Quick Look, Copy Path, Move to Trash, Run in Chat…), and
  their shortcuts work straight from the list. A preview pane shows files,
  clipboard items, snippets, notes, chats and command details.
- **Saved chats and attachments**: every chat is saved (`chats …` to find
  one); attach files, PDFs, the clipboard or the screen (⌘O / ⌘⇧V / ⌘⇧S).
  A vision model (`gpt-4o`, `claude…`, `gemini…`, `…-vl`) gets the pixels; any
  other model gets text read out on-device, so screenshots work everywhere.
- **Snippets that expand anywhere**: type `;sig` in any app.
- **Search Menu Items** of the front app, **My Schedule** with one-key join
  for Zoom/Meet/Teams, **dictation** (⌘D, on-device).
- **Settings window** with Keychain key entry and a permissions checklist;
  `config.toml` stays the source of truth and reloads itself when it changes.
- **Sync** between Macs through iCloud Drive (config, scripts, notes,
  layouts, pins — never clipboard history).
- **Private by default**: no telemetry; the only network traffic is to the AI
  and MCP endpoints in your config. Keys stay in the Keychain (or any secret manager).

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

The first launch opens **Settings → Permissions**, which shows what is granted
and asks for the rest. Each one is only needed by the features listed:

| Permission | Used by |
| --- | --- |
| Accessibility | window management and layouts, AI on selected text, paste, snippet expansion, menu search |
| Screen Recording | Click Text, attaching screen text to a chat |
| Calendars | My Schedule, the next-meeting row |
| Microphone + Speech Recognition | dictation |

Window moves go through the Accessibility API directly (no AppleScript, no
subprocess), so a snap takes milliseconds.

### From a script or another app

```bash
open "opencast://search?q=coffee"          # open the panel with this query typed
open "opencast://run/builtin:reload"       # run a command by its id
open -a OpenCast Workflow.alfredworkflow   # import an Alfred workflow or snippets
```

One OpenCast runs at a time: a second copy says so and quits, because two panels
would fight over the same global hotkeys.

## Windows and layouts

Type "half", "maximize", "third"… in the launcher to snap the window you were
just using (Rectangle-style). Full screen has its own rows too — **Full Screen**,
**Exit Full Screen** and the **Toggle Fullscreen** you may already have bound —
so a hotkey does the one thing you expect and says so when it can't.
**Spaces** (Mission Control desktops): **Switch Space** lists every desktop with
the apps that live there — Return moves the view, and each desktop also gets a
**Move Window to Space N** row that takes the window with you. **Next Space**,
**Previous Space** and **Move Window to Next/Previous Space** are commands you
can bind. A full-screen window owns a space of its own: the list shows it, and
moving a window skips past those. Switching is a synthesized Dock swipe and
moving is SkyLight's own operation, so neither needs the Accessibility
permission; both are private APIs, so a macOS that stops answering them reports
an error rather than leaving a window half-moved.
**Remember Layout** saves every visible window's
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

Window keys come from a preset, and the config you start with enables
Rectangle's:

```toml
window_hotkeys = "rectangle"   # ⌃⌥ + arrows for the halves, U I J K for the
                               # corners, D E F T G for the thirds, ⌃⌥⌅ to
                               # maximize, ⌃⌥⌫ to put it back, ⌃⌥= / ⌃⌥- bigger
                               # and smaller, ⌃⌥⌘ + arrows for displays
```

`"spectacle"` installs the ⌥⌘ set Rectangle registers out of the box (and that
Spectacle used before it); `"off"` binds nothing. Preset names combine with a
comma: `"rectangle, spaces"` adds the Mission Control keys (⌃⌥⇧ + arrows for
previous / next space) to the tiling set. The `spectacle` set uses those same two
keys for make-smaller / make-larger, so with both named the preset written first
keeps them and the launcher says which command was left without a key. Use `"off"` while Rectangle,
Magnet or Hammerspoon is still running — two apps registering the same keys means
only one of them gets the press. Your own bindings always win: a `[[hotkeys]]`
entry on the same keys, or a rebind in the Window tab, replaces just that one
preset entry. The code default is `"off"`; the preset is opt-in for anyone whose
config predates it.

Window behaviour lives in `[general]`: `window_gap` and `screen_margin`
(points), `window_cycle` (`sizes`: pressing Left Half again goes ½ → ⅔ → ⅓;
`displays`: it moves to the next screen; `off`), `almost_maximize` (0.9),
`window_portrait`,
`window_hud` and `drag_snap`. Drag to snap stays off while macOS's own
edge tiling (System Settings → Desktop & Dock) is on.

Rotated (portrait) monitors are detected from the screen's shape. There,
First/Center/Last Third, the two-thirds and Center Half stack top-to-bottom,
so *Center Third* puts a window in the vertical middle. Commands named for a
side or corner keep their meaning: the corner sixths use a 2 × 3 grid (with
*Middle Left/Right Sixth* for the middle row), so Bottom Left Sixth is always
bottom left. Icons
and the Arrange Window grid turn tall and list the stacking commands first,
and drag to snap uses the top and bottom edges (⌥ for thirds). *Top/Middle/
Bottom Third*, *Top/Bottom Two Thirds* and *Middle Half* do the same on any
screen. `window_portrait = "off"` keeps columns everywhere. Add your own positions:

```toml
[[window_commands]]
id = "code-left"
title = "Code Left"
x = 0
y = 0
w = 0.62          # fraction of the screen, or "800px"
h = 1.0
hotkey = "Ctrl+Alt+C"
```

**Remember Layout** asks which apps to keep before saving, and **Restore
Layout** places each app as soon as its first window appears.

`hyperkey = true` in `[general]` turns Caps Lock into a hyper key (restart
OpenCast to apply): held, it adds ⌃⌥⌘⇧ to whatever you press, so `Hyper+K`
shortcuts work in any app and in `[[hotkeys]]`; tapped alone, it opens the
launcher. OpenCast remaps Caps Lock with `hidutil` on launch and undoes it on
*Quit OpenCast*. The bundled *Use Caps Lock as Launcher Key* script is the
hyper-less version: Caps Lock → F18, then `hotkey = "F18"`.

*Click Text* is Superkey's Seek: it reads the screen with macOS's on-device
OCR and overlays a search pill — type what you see, every match is
highlighted (a dashed line leads from the search field to the current one),
`;` or Tab cycles, Enter clicks, Esc cancels. Bind it to
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

Prompt placeholders: `{selection}` (selected text in the frontmost app),
`{input}` (asked for after you pick the command), `{clipboard}` and `{app}`
(the frontmost app's name).

Per command, `temperature` and `max_tokens` override the provider, and
`output` decides where the answer goes: `chat` (default, opens the chat
panel), `replace` (streams silently, then pastes over the selection — the
"fix my text in place" feel), `paste` (into the front app) or `copy`
(clipboard + toast).

After editing, run **Reload Commands** from the launcher.

## Customizing commands

`[[commands]]` tweaks any command, apps and builtins included:

```toml
[[commands]]
match = "Slack"              # command id or title, case-insensitive
alias = "sl"                 # an exact alias match ranks first
aliases = ["slk"]
icon = "bubble.left.fill"    # emoji or an SF Symbol name
title = "Slack (work)"
boost = 50                   # added to the search score; negative demotes
hidden = false               # true removes it from search
```

**Fallbacks** are what you see when a query has fewer than
`fallback_threshold` strong hits, in the order you list them:

```toml
[general]
fallbacks = ["Search Kagi", "builtin:chat", "files"]   # ids/titles, or the tokens
fallback_threshold = 3                                  # "quicklinks" and "files"
```

**Keys** inside the panel live in `[keys]` (`ask_ai`, `click_text`, `actions`,
`pin`, `paste_answer`, `copy_answer`, `new_chat`, `next`, `previous`), as
`Cmd+Shift+P`-style specs.

A **theme file** (`theme_file = "themes/mine.toml"`, see
`examples/themes/warm.toml`) sets the accent, panel and text colours, corner
radius, row height, width, font sizes, font family and whether the panel is
glass or opaque.

## Calculator

Type maths and the first row is the result; Return copies it.

| | |
| --- | --- |
| `2 * (3 + 4) ^ 2 / 7`, `1,200 x 3`, `10 % 4` | arithmetic, thousands separators, modulo |
| `sqrt 2`, `sin(pi/2)`, `log(100)`, `ln e`, `round(2.5)` | functions and constants |
| `0x1f + 0b101` | hex and binary |
| `15% of 80`, `80 + 15%`, `80 - 15%`, `80 * 15%` | percentages |
| `10 km in mi`, `72 f to c`, `3 gb in mb`, `2 h in min`, `5 lb in kg` | units: length, mass, volume, data, time, speed, area, temperature |

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
(`general.script_timeout_secs`), through your login shell so Homebrew, nvm
and friends are on `PATH` (`script_login_shell = false` to opt out).
`chmod +x` is required. `OPENCAST=1` and `OPENCAST_FRONT_APP` are set.

### Script Filters (`@opencast.mode list`)

A list-mode script prints a JSON array; the panel shows the rows and Return
re-runs the script with `OPENCAST_ACTION=run` and the chosen `arg` as `$1`:

```bash
#!/bin/bash
# @opencast.title Recent Branches
# @opencast.mode list
# @opencast.action_mode compact     # how to treat the run step's output
if [ "$OPENCAST_ACTION" = "run" ]; then git checkout "$1" && echo "Switched to $1"; exit; fi
git branch --sort=-committerdate --format='%(refname:short)' | head -20 \
  | jq -R '{title: ., subtitle: "git checkout", arg: .}' | jq -s .
```

Items are `{title, subtitle?, icon?, arg?, autocomplete?}` (or plain
strings). OpenCast filters the rows as you type; `@opencast.filter false`
re-runs the script with the query as `$1` instead (for scripts that search
something). `examples/scripts/recent-branches.sh` is the bundled example.

## Keys

| Where | Key | Does |
| --- | --- | --- |
| Search | ↑ ↓ / ⌃N ⌃P | move |
| | ↵ | run / open / copy calculator result |
| | ⇥ | ask AI about the typed text |
| | ⌘K | actions for the selected row (their shortcuts also work directly) |
| | ⌘Y | Quick Look |
| | ⌘I | show / hide the preview pane |
| | ⌘D | dictate |
| | ⌘L | Large Type (the typed text or the selected answer) |
| | ⌘⇧P | pin / unpin |
| | ` | Click Text |
| | esc | clear, then close |
| Chat | ↵ / ⇧↵ | send / new line |
| | ⌘O / ⌘⇧V / ⌘⇧S | attach a file / the clipboard / the screen's text |
| | ⌘↵ | paste last answer into the previous app |
| | ⌘⇧C | copy last answer |
| | ⌘N | new chat |
| | ↵ / esc on a tool prompt | allow / deny the tool call |
| | esc | stop streaming, then back |

All of these can be changed under `[keys]` in `config.toml`.

## MCP servers

```toml
[[mcp_servers]]
id = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env_cmd = { GITHUB_PERSONAL_ACCESS_TOKEN = "security find-generic-password -s opencast-github -w" }
auto_approve = ["get_*", "list_*", "search_*"]   # the AI may run these without asking
```

Run **Refresh MCP Tools** (or just save the file — config changes reload
automatically). Each tool becomes a command ("Create Issue", "Search Code"…)
whose argument form comes from the tool's schema, and the AI chat can call the
tools: it shows each call and asks before running anything not in
`auto_approve`. Servers that also advertise **resources** get one command per
document (Return reads it into the panel; the URI shows in the preview) and
**prompt templates** get one command each (fill the arguments, and the server's
own wording opens in a chat, unsent, ready to edit). Stdio servers (`command`)
and Streamable HTTP servers (`url`, with `headers` / `headers_cmd`) are
supported; OAuth-only remote servers are
not yet.

## Shortcuts, snippets, schedule, menus

- **Shortcuts.app** shortcuts appear as commands. ⌘K → *Run with Clipboard as
  Input* passes the clipboard text in; text output is shown.
- **Snippets** whose keyword starts with a symbol (`;sig`, `::addr`) expand as
  you type in any app; others are found by searching. Placeholders:
  `{date}`, `{time}`, `{datetime}`, `{clipboard}`, `{cursor}`.
- **My Schedule** lists today's and tomorrow's events; Return joins a video
  call or opens the event. When a meeting is within the hour it is the first
  row of the empty launcher.
- **Search Menu Items** lists every menu command of the app you were in, with
  its shortcut. Bind it to a hotkey for a command palette in every app.

## Extensions and Alfred workflows

- **Install Extension** takes `owner/repo` (GitHub), one folder of a big repo
  (`raycast/script-commands/commands/apps/spotify`), a `…/tree/<branch>/<folder>`
  URL or any git URL. Extensions are plain clones in
  `~/.config/opencast/extensions/`; their OpenCast or Raycast script commands
  show up like your own.
- **Browse Extensions** lists curated folders of Raycast's script-command
  collection and GitHub repos tagged `opencast-extension`. Return installs.
- **Update Extensions** pulls them all; **Manage Extensions** shows what is
  installed (⌘K → Remove).
- **Import Alfred Workflow or Snippets** (or Finder › Open With › OpenCast on
  an `.alfredworkflow` or `.alfredsnippets`). Keyword and Script Filter inputs
  become commands and run the workflow's own scripts with Alfred's environment
  and data folders. Hotkey triggers become commands with that global hotkey
  (selection, clipboard or text as input; unset hotkeys can be recorded with
  ⌘K › Record Hotkey). Connected Run Script, Open URL, Copy to Clipboard,
  Notification, Large Type, Arg and Vars, Junction, Conditional and Transform
  steps work; ⌘/⌥/⌃ modifier branches too. Other step types say so when
  reached. A workflow with settings gets **Configure \<workflow\>**: type a
  value, Return on the setting. Snippet collections are added to
  `[[snippets]]` with the collection's keyword prefix/suffix.
- Any script can declare `# @opencast.hotkey Ctrl+Alt+K`.

### Richer Script Filter output

List scripts can print more than rows (Alfred's Script Filter JSON is accepted
as is):

```json
{
  "view": "grid", "columns": 5, "rerun": 1.0,
  "items": [{
    "title": "Red", "subtitle": "#ff0000", "arg": "#ff0000",
    "icon": "🟥",                         // emoji, SF Symbol, image path, {"path": …}
    "markdown": "# Red\nShown in the detail pane",
    "actions": [{"title": "Copy RGB", "arg": "rgb(255,0,0)", "key": "cmd+shift+c"}],
    "quicklook": "/path/or/url", "copy": "#ff0000",
    "valid": true, "variables": {"KEY": "passed to the run step as env"}
  }]
}
```

⌘K on a row lists its actions; their keys work straight from the list.

## Built-ins

- **Conversions**: `10 km in mi` and other units (calculator), `time in tokyo`,
  `3pm pst in kyiv`, `9am london` (to your zone), and with `currency = true`
  under `[general]`, `100 usd to eur` / `€50 in uah` (rates fetched daily).
- **System Settings panes**: type `bluetooth`, `wi-fi`, `privacy`,
  `full disk access`, `displays`…
- **Bookmarks** from Safari, Chrome, Brave, Edge, Arc and Vivaldi: a few match
  any search; `bm …` lists only bookmarks. (Safari needs Full Disk Access.)
- **Search Contacts** (⌘K: email, call, message, copy), **Reminders** (Return
  completes), **New Reminder** (`when` understands “tomorrow 9am”).
- **Define Word** / `define serendipity` (macOS dictionary), **Large Type**.
- **Play / Pause**, **Next Track**, **Previous Track** for whatever is playing.
- **Act on Selection** (`Hyper+Space` by default, `universal_actions_hotkey`):
  actions for the selected text, link or Finder files: Ask AI, AI commands,
  Define, Spell, Search the Web, Large Type, Save as Note; Open in each browser,
  Copy as Markdown Link; Open With, Quick Look, Reveal, Copy Path/File, Open in
  Terminal, Email, Compress, Move To / Copy To, Trash. ⌘K on a file row shows the
  same list. Scripts join with `# @opencast.accepts text, url, file` (input as
  `$1`, files as separate arguments), AI commands with `accepts = ["url"]`
  (`{selection}` prompts accept text by default). The order learns per type.
- **Path mode**: type `/` or `~/`. Tab or → enters a folder, ⌘↑ goes up,
  ⌘⇧. shows hidden files. **Browse Files** reopens the last folder.
- **File Buffer**: ⌥↑ adds the selected file (from path mode, file search or
  clipboard), ⌥↓ removes it, ⌥← clears. ⌘K and Act on Selection then act on
  all of them.
- **Start Focus** / **Pause / Resume Focus** / **Stop Focus**: hides the
  `[focus]` apps and closes tabs on `[focus] domains` in Safari and
  Chrome-family browsers for 25 minutes (or the minutes you type, with an
  optional goal). The footer and a menu bar item show the time left.
- **Notes Editor**: a notes window with search and autosave. Return on a note
  in the launcher opens it there; ⌘K → Paste pastes it. ⌘⇧P floats it above
  other windows, ⌘⇧M previews Markdown, ⌘⇧O pops the note out as a floating
  sticky (several can be open). **New Sticky Note** starts one directly.
- **Translate**: `tr hola`, `translate good morning to German`,
  `translate to french: see you`, or **Translate** on the selection (also in
  Act on Selection). Uses your AI provider; `translate_to` / `translate_alt`.
- **Ask About This Page** / **Summarize This Page**: the front tab of Safari,
  Chrome, Arc, Brave, Edge or Vivaldi becomes chat context. With the browser's
  *Allow JavaScript from Apple Events* on, the open tab's text is used (works
  for logged-in pages); otherwise OpenCast downloads the URL.
- **Compare Models**: one prompt to up to four provider/model pairs side by
  side, with time to first token, total time and words per second.
- **Dictate Anywhere**: bind a hotkey; speak, and the text is pasted into the
  front app when you pause (on-device recognition; Esc cancels).
- **Keyboard Shortcuts**: pick a running app, then pick one of its menu
  shortcuts and it runs in that app. The search bar filters at every level — the
  app list, then that app's shortcuts, fuzzy, and `⌘t` finds ⌘T — while Esc
  steps back a level instead of closing. **Keyboard Shortcuts** is also an action
  on any running app's row, and **Search Menus** does the same for every menu
  item, key equivalent or not.
- **Pipelines** (`[[pipelines]]`): run the selection, clipboard or typed text
  through AI commands, ad hoc prompts, scripts, shell commands and built-in
  transforms (trim, sort-lines, json-format, base64…), then show, copy, paste
  or replace. Selection pipelines appear in Act on Selection.
- **Global hotkeys for any command**: ⌘K → *Record Hotkey…* on any row, or
  `hotkey = "Hyper+V"` in a `[[commands]]` entry. Clashes show as warnings.
- **System**: Restart, Shut Down, Log Out, Quit All Apps (these ask first;
  `confirm_destructive`), Toggle Hidden Files, Hide All Apps Except Front, Show
  Desktop, plus Lock, Sleep, Empty Trash, Eject, volume and brightness.
- **Spotify**: **Spotify** opens the player's own list — the current track and
  where you are in it, play/pause, next, previous, seek ±10s, volume, and Now
  Playing (Return there copies the track line). **Spotify Play / Pause**,
  **Spotify Next Track**, **Spotify Previous Track** and **Spotify Now Playing**
  are commands of their own, so a hotkey can drive Spotify even when a browser tab
  owns Now Playing. No shuffle or repeat rows: Spotify ignores those script writes
  and has no menu item for them, so the launcher leaves them out.
- **Emoji**: every Unicode emoji by name (`emoji pretzel`), recent ones first,
  `emoji_skin_tone` applied.
- **AI presets** (`[[ai_presets]]`): a chat with its own model, prompt and MCP
  tool globs, shown as a command. **Edit AI Profile** opens `profile.md`, which
  is added to chats (`ai_profile`); ⌘⇧R in chat remembers a line.
- **Shared config**: `include = ["~/team/opencast.toml", "git:<url>"]` merges
  snippets, quicklinks, commands, AI commands and presets; local entries win.
- **Spell** (`spell recieve`), **OpenCast Stats**, **Save as Snippet** (⌘S on a
  clipboard text), colour swatches for colour clips, **Recent Documents** (⌘R on
  an app), Shift-tap Quick Look (`shift_quick_look`), `sync_dir` for Sync Config.
- **Scripts** get `OPENCAST_DATA` / `OPENCAST_CACHE` and Alfred's `alfred_*`
  variables; `@opencast.mode markdown` renders output as Markdown and
  `@opencast.mode grid` shows list rows as a grid.
- **OpenCast Doctor** (or `./smoke.command` in a terminal): checks the config
  folder for oversized or repeated files, keys and section names nothing reads
  (typos in `config.toml` are usually silent), warnings, providers without keys,
  MCP servers that never connected, and search speed with your own apps.

## Releasing

`./release.command` builds **both arch slices** (arm64 + x86_64, lipo'd into one
universal binary, so a single dmg runs on any Mac), signs with your Developer ID
(hardened runtime), notarizes, staples and packages a `.dmg` and `.zip`, then
prints the Homebrew cask with version, `sha256` and owner/repo filled in from
your git remote. One-time setup and the cask template are in `release.command`
and `packaging/`; `packaging/build-arch.zsh` builds one slice, which is also what
CI runs for the Intel arch. `OC_ARCH=arm64` builds a single slice, and
`OC_ARCH=universal ./build-app.command` does the same for your local install.
**Check for Updates** looks at the latest GitHub release of `update_repo`; set
`check_updates = true` for a quiet weekly check.

## Layout

```
crates/opencast-core/   portable logic: config, registry + fuzzy search,
  src/providers/          scripts, app discovery, calculator, AI adapters (+ tests)
crates/opencast-ffi/    macOS C ABI: hotkeys, clipboard, paste, Seek OCR, app index,
                          Shortcuts, MCP hub + AI tool loop (agent.rs)
macos/                  SwiftUI panel, settings window, window manager (AX),
                          menu search, dictation, calendar, snippet expansion
packaging/              entitlements, the Homebrew cask template, build-arch.zsh
                        (one arch slice: Rust engine + Swift panel)
examples/               default config.toml and example scripts
```

The panel never holds keys or talks to the network; it calls the Rust engine
and receives streamed tokens from it.

## Status and roadmap

v0.2. Next: OAuth for remote MCP servers, browser history, Raycast extension
(React) compatibility, and Linux packaging (the core is portable).

Prior art worth knowing: [SuperCmd](https://github.com/SuperCmdLabs/SuperCmd)
(Electron, Raycast-extension compatible) and
[Vicinae](https://docs.vicinae.com) (Linux, Qt).

## About

OpenCast is an independent project, not affiliated with or endorsed by
Raycast Technologies Ltd. or Running with Crayons Ltd. Raycast and Alfred are
trademarks of their owners. OpenCast runs script commands and workflows built
for other launchers; it does not use their services.

Contributions are welcome: see [CONTRIBUTING.md](CONTRIBUTING.md) for the layout
and the rules, [CHANGELOG.md](CHANGELOG.md) for what changed, and
[SECURITY.md](SECURITY.md) for the threat model (a launcher runs your scripts as
you) and how to report a problem privately.

## License

MIT
