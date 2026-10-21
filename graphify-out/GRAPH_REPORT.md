# Graph Report - opencast  (2026-10-20)

## Corpus Check
- 59 files · ~49,577 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1071 nodes · 2299 edges · 55 communities (49 shown, 6 thin omitted)
- Extraction: 98% EXTRACTED · 2% INFERRED · 0% AMBIGUOUS · INFERRED: 56 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `941ece2f`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Model
- ChatView
- config.rs
- ExecOutcome
- Engine
- window_mgmt.rs
- commands.rs
- apps.rs
- scripts.rs
- mod.rs
- host.rs
- layouts.rs
- openai.rs
- stream.rs
- SeekView.swift
- Parser
- anthropic.rs
- gemini.rs
- SwiftUI panel, Rust engine
- Gaps
- View
- SseEvent
- File map
- Raycast OOTB parity analysis
- OpenCast
- platform.rs
- Global Constraints
- StayAwakeController
- Speed, presentation, and theme
- OpenCast — project notes for Claude Code
- Stay Awake (Amphetamine-lite)
- Island, app index, and click session
- capslock-launcher-key.sh
- ip-address.sh
- toggle-dark-mode.sh
- uuid.sh
- Theme
- Adaptive glass panel
- pins.rs
- usage.rs
- Global Constraints
- ClipboardHistory
- to_commands
- notes.rs
- 2026-10-20-pro-parity-slice-design.md
- String
- LauncherView
- pump
- Endpoint
- 2026-10-20-smart-external-search-design.md
- Light text colors and icons
- files_mac.rs
- Clipboard + screenshots (Raycast/Alfred practices)
- LabelKind

## God Nodes (most connected - your core abstractions)
1. `Model` - 106 edges
2. `ClipboardHistory` - 40 edges
3. `Engine` - 28 edges
4. `ExecOutcome` - 25 edges
5. `Theme` - 25 edges
6. `Entry` - 23 edges
7. `LauncherView` - 21 edges
8. `Config` - 20 edges
9. `Kind` - 17 edges
10. `StayAwakeController` - 17 edges

## Surprising Connections (you probably didn't know these)
- `.body` --calls--> `HintBar`  [INFERRED]
  macos/ChatView.swift → macos/LauncherView.swift
- `delete()` --calls--> `slug()`  [INFERRED]
  crates/opencast-core/src/layouts.rs → crates/opencast-core/src/config.rs
- `save()` --calls--> `slug()`  [INFERRED]
  crates/opencast-core/src/layouts.rs → crates/opencast-core/src/config.rs
- `run_command()` --calls--> `fill_selection()`  [INFERRED]
  crates/opencast-ffi/src/exec.rs → crates/opencast-core/src/engine.rs
- `anthropic_stream()` --calls--> `complete()`  [INFERRED]
  crates/opencast-core/tests/stream.rs → crates/opencast-core/src/providers/mod.rs

## Import Cycles
- None detected.

## Communities (55 total, 6 thin omitted)

### Community 0 - "Model"
Cohesion: 0.05
Nodes (48): .header, ActionRow, IslandPanel, .canBecomeKey, .canBecomeMain, Mode, actions, args (+40 more)

### Community 1 - "ChatView"
Cohesion: 0.10
Nodes (28): Identifiable, AssistantBody, .body, Block, ChatView, .body, .emptyHint, .messages (+20 more)

### Community 2 - "config.rs"
Cohesion: 0.11
Nodes (31): AiCommandConfig, api_key_from_cmd_and_models_dedupe(), Config, default_hotkey(), default_presentation(), default_theme(), default_timeout(), empty_config_is_valid() (+23 more)

### Community 3 - "ExecOutcome"
Cohesion: 0.06
Nodes (45): CryptoKit, Equatable, Foundation, Kind, opencastCall(), String, Argument, ChatTurn (+37 more)

### Community 4 - "Engine"
Cohesion: 0.07
Nodes (37): ai_commands_fill_input_and_flag_selection(), Builtin, Engine, engine_with(), fill_selection(), host_of(), hotkeys_resolve_titles_and_ids(), LayoutOp (+29 more)

### Community 5 - "window_mgmt.rs"
Cohesion: 0.12
Nodes (34): WindowAction, adjacent_display_rect(), apply_layout(), apply_window_action(), ApplyResult, capture_layout(), click_at(), cursor_monitor() (+26 more)

### Community 6 - "commands.rs"
Cohesion: 0.09
Nodes (32): Argument, calculator_row_comes_first(), category_ai_prefix(), category_layout_beats_unrelated_apps(), category_prefix_promotes_window_commands(), cmd(), Command, CommandKind (+24 more)

### Community 7 - "apps.rs"
Cohesion: 0.16
Nodes (26): AppEntry, filter_mac_apps(), info_plist_names_and_background(), is_nested_app(), launch(), mac_scan_dedupes_by_name(), MacAppHint, nested_app_and_background_are_dropped() (+18 more)

### Community 8 - "scripts.rs"
Cohesion: 0.17
Nodes (22): Argument, discover(), discover_and_run(), is_executable(), OutputMode, parse_header(), parses_opencast_header(), parses_raycast_header_with_trailing_comment_and_args_in_order() (+14 more)

### Community 9 - "mod.rs"
Cohesion: 0.21
Nodes (19): build(), ChatRequest, complete(), http_error(), http_error_extracts_message(), Message, Provider, ProviderInfo (+11 more)

### Community 10 - "host.rs"
Cohesion: 0.07
Nodes (40): err(), ExecResult, hide(), reload(), Command, Option, String, Vec (+32 more)

### Community 11 - "layouts.rs"
Cohesion: 0.25
Nodes (20): AppWindows, delete(), empty_or_missing_store_is_fine(), file(), get(), Layout, list(), load() (+12 more)

### Community 12 - "openai.rs"
Cohesion: 0.15
Nodes (10): body(), body_shape(), OpenAi, Provider, Result, Sender, String, Value (+2 more)

### Community 13 - "stream.rs"
Cohesion: 0.47
Nodes (9): anthropic_stream(), gemini_stream_without_terminator(), http_errors_become_readable_events(), openai_compatible_stream(), req(), router_for(), String, serve_once() (+1 more)

### Community 14 - "SeekView.swift"
Cohesion: 0.22
Nodes (18): .seekSpans, cg(), charX(), jsonInt(), matchSpans(), SeekHit, SeekRect, SeekScan (+10 more)

### Community 15 - "Parser"
Cohesion: 0.29
Nodes (8): eval(), format_number(), Parser, Option, String, Vec, Tok, tokenize()

### Community 16 - "anthropic.rs"
Cohesion: 0.15
Nodes (10): Anthropic, body(), Provider, Result, Sender, String, Value, Vec (+2 more)

### Community 17 - "gemini.rs"
Cohesion: 0.15
Nodes (10): body(), Gemini, Provider, Result, Sender, String, Value, Vec (+2 more)

### Community 18 - "SwiftUI panel, Rust engine"
Cohesion: 0.12
Nodes (16): Args, output, chat, Build, C ABI, Deletions, Error handling, Goal, Icons, Launcher panel (+8 more)

### Community 19 - "Gaps"
Cohesion: 0.13
Nodes (14): Cheap partials, Clipboard persistence, Deep links, Emoji picker, Fallback search, Favorites / pinned, File search, Gaps (+6 more)

### Community 20 - "View"
Cohesion: 0.17
Nodes (20): ActionsPane, .actions, .scheme, ArgsForm, .body, .hairline, .scheme, BackButton (+12 more)

### Community 21 - "SseEvent"
Cohesion: 0.36
Nodes (8): multiline_data_and_split_utf8(), Option, Self, String, Vec, splits_across_chunks_and_crlf(), SseEvent, SseParser

### Community 22 - "File map"
Cohesion: 0.14
Nodes (13): File map, Global Constraints, SwiftUI Panel Implementation Plan, Task 10: Bundle script, docs, and deleting the web shell, Task 1: Drop the repeated subtitles, Task 2: JSON dispatch for the read-only ops, Task 3: C ABI, callback, and startup, Task 4: Move the macOS glue off Tauri (+5 more)

### Community 23 - "Raycast OOTB parity analysis"
Cohesion: 0.17
Nodes (11): AI, Clipboard & text, Core launcher, Extensibility, OpenCast extras (not Raycast), OpenCast-only (ahead of Raycast), Raycast OOTB parity analysis, Recommended parity phases (+3 more)

### Community 24 - "OpenCast"
Cohesion: 0.18
Nodes (10): Configuring providers, Keys, Layout, License, macOS permissions, OpenCast, Quick start, Script commands (+2 more)

### Community 25 - "platform.rs"
Cohesion: 0.50
Nodes (8): grab_selection(), keystroke(), paste_into_frontmost(), read_clipboard(), Option, Result, String, write_clipboard()

### Community 26 - "Global Constraints"
Cohesion: 0.22
Nodes (8): Global Constraints, Spec coverage, Speed, presentation, and theme Implementation Plan, Task 1: Full macOS app index, Task 2: Presentation island | spotlight, Task 3: Theme system | light | dark + Mac accent, Task 4: Hotkey on rows + Shortcuts command, Task 5: Build and install

### Community 27 - "StayAwakeController"
Cohesion: 0.08
Nodes (25): CChar, Content, Send, start(), Fn, AppDelegate, ClearHost, .isOpaque (+17 more)

### Community 28 - "Speed, presentation, and theme"
Cohesion: 0.29
Nodes (6): 1. Speed and full app index, 2. Presentation: island or Spotlight, 3. Theme and hotkey map, Hotkeys on rows, Searchable shortcuts map, Speed, presentation, and theme

### Community 29 - "OpenCast — project notes for Claude Code"
Cohesion: 0.40
Nodes (4): Build / test, Conventions, Layout, OpenCast — project notes for Claude Code

### Community 30 - "Stay Awake (Amphetamine-lite)"
Cohesion: 0.33
Nodes (5): Commands, Mechanism, Menu bar, Out of scope (v1), Stay Awake (Amphetamine-lite)

### Community 31 - "Island, app index, and click session"
Cohesion: 0.40
Nodes (4): Apps, Click, Island, Island, app index, and click session

### Community 37 - "Theme"
Cohesion: 0.17
Nodes (16): .field, .body, .body, .body, .body, .field, .list, Bool (+8 more)

### Community 38 - "Adaptive glass panel"
Cohesion: 0.33
Nodes (5): Adaptive glass panel, Material, Out of scope, Polish (same pass), Shapes

### Community 39 - "pins.rs"
Cohesion: 0.32
Nodes (11): boost(), load(), load_store(), path(), HashSet, PathBuf, String, Vec (+3 more)

### Community 40 - "usage.rs"
Cohesion: 0.42
Nodes (11): boost(), bump(), bump_and_boost(), counts(), load(), path(), HashMap, PathBuf (+3 more)

### Community 41 - "Global Constraints"
Cohesion: 0.22
Nodes (8): Global Constraints, Parity gaps (B) Implementation Plan, Task 1: Pins + usage-aware empty query, Task 2: System builtins (volume, brightness, dark mode, quit apps), Task 3: Emoji picker, Task 4: File search content + fallback quicklinks, Task 5: Clipboard persist + deep links, Task 6: Build + install

### Community 42 - "ClipboardHistory"
Cohesion: 0.07
Nodes (35): Codable, CodingKey, Data, Decoder, Encoder, ClipboardHistory, CodingKeys, at (+27 more)

### Community 43 - "to_commands"
Cohesion: 0.24
Nodes (10): builds_file_commands(), Command, Item, Path, PathBuf, String, Vec, shorten_home() (+2 more)

### Community 44 - "notes.rs"
Cohesion: 0.28
Nodes (15): add(), add_and_match(), get(), load_store(), matching(), Note, now(), path() (+7 more)

### Community 46 - "String"
Cohesion: 0.20
Nodes (8): AppKit, EnvironmentKey, .argIcon, EnvironmentValues, .themePreference, Command, String, ThemePreferenceKey

### Community 47 - "LauncherView"
Cohesion: 0.17
Nodes (12): AnyShape, .hint, LauncherView, .body, .footer, .isSpotlight, .panelFill, .panelShape (+4 more)

### Community 48 - "pump"
Cohesion: 0.27
Nodes (10): parse(), parse(), ChatEvent, forward(), pump(), Sender, Step, parse() (+2 more)

### Community 49 - "Endpoint"
Cohesion: 0.24
Nodes (6): Client, ProviderKind, default_base_url(), Endpoint, HashMap, RequestBuilder

### Community 51 - "Light text colors and icons"
Cohesion: 0.29
Nodes (6): Approach, Icons, Light text colors and icons, Out of scope, Problem, Text

### Community 52 - "files_mac.rs"
Cohesion: 0.61
Nodes (8): content_search(), filter_paths(), name_search(), Command, PathBuf, Vec, run_mdfind(), search()

### Community 53 - "Clipboard + screenshots (Raycast/Alfred practices)"
Cohesion: 0.29
Nodes (6): Capture, Clipboard + screenshots (Raycast/Alfred practices), Goal, Out of scope, Store (`~/.config/opencast/`), UI / commands

### Community 54 - "LabelKind"
Cohesion: 0.50
Nodes (4): LabelKind, label, secondary, tertiary

## Knowledge Gaps
- **179 isolated node(s):** `SeekWord`, `capslock-launcher-key.sh script`, `ip-address.sh script`, `toggle-dark-mode.sh script`, `uuid.sh script` (+174 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **6 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Model` connect `Model` to `ChatView`, `ExecOutcome`, `Theme`, `ClipboardHistory`, `SeekView.swift`, `LauncherView`, `View`, `StayAwakeController`?**
  _High betweenness centrality (0.081) - this node is a cross-community bridge._
- **Why does `Engine` connect `Engine` to `mod.rs`, `config.rs`, `host.rs`, `commands.rs`?**
  _High betweenness centrality (0.052) - this node is a cross-community bridge._
- **Why does `Router` connect `mod.rs` to `Endpoint`, `Engine`, `stream.rs`?**
  _High betweenness centrality (0.031) - this node is a cross-community bridge._
- **Are the 4 inferred relationships involving `Model` (e.g. with `.messages` and `.body`) actually correct?**
  _`Model` has 4 INFERRED edges - model-reasoned connections that need verification._
- **What connects `SeekWord`, `capslock-launcher-key.sh script`, `ip-address.sh script` to the rest of the system?**
  _179 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Model` be split into smaller, more focused modules?**
  _Cohesion score 0.05007935108446482 - nodes in this community are weakly interconnected._
- **Should `ChatView` be split into smaller, more focused modules?**
  _Cohesion score 0.10080645161290322 - nodes in this community are weakly interconnected._