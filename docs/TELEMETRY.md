# Telemetry

Local only. The logger writes files under the home directory and keeps a ring in memory. It does not open a socket.

## Files

`~/Library/Logs/OpenCast/`

- `opencast.log` and dated rotations. Kept for 7 days, and the directory is pruned at 32 MB. The newest file is kept when it is the only one.
- `panic.log` is appended and fsynced from the panic hook.

## Level

`OPENCAST_LOG` is a `tracing` env filter. The default is `info`.

Settings → diagnostic logging forces `debug` while the toggle is on, including over an env filter set at launch. Turning the toggle off sets `info`.

Hot ops (`search`, `listPath`, `rankActions`, `bumpUsage`, `preview`, `hidden`, `shown`, `windowTarget`, `windowGlyphs`, `debugSnapshot`, `settings`, `snippets`, `apps`, `accepting`) are recorded as `TRACE` spans, so they stay out of the file at `info`. Histograms are recorded either way, which is how p95 exists without debug logging.

A sample over its budget logs a warning.

## Samples

| Name | Where | Budget |
|---|---|---|
| `search` | Rust `Engine::search` | 16 ms |
| `keystroke` | Swift, from the keystroke to rows delivered | 16 ms plus the debounce (50 ms while typing, 0 when immediate) |
| `panel.show` | Swift, through `orderFront` of the on-screen frame | 50 ms |
| `window.snap` | Swift `WindowManager.apply` | 30 ms |
| `startup` | Swift, from before `opencast_start` to `Model.ready` | 400 ms |
| `hotkey` | Rust hotkey callback | 50 ms |
| `config.write`, `layouts.load`, `layouts.write` | Rust | 100 ms |
| `execute`, `script`, `extension` | Rust | recorded, budget 0 |

The query string is not a span field. At `debug` it is logged clipped to 200 characters.

## Debug command

The builtin command **Debug** opens a window that reads the ring: level, recent lines, per-op percentiles, slow samples, hotkeys, RSS, and CPU. Its timer runs only while the window is open. Export writes a zip in the temp directory with the log files, `system.txt`, and `config.toml`.

Config lines whose keys contain `api_key`, `apikey`, `token`, `secret`, `password`, or `authorization` are written as `[redacted]`. Log lines that mention those words have quoted values scrubbed.

## Panic

The hook writes the payload and a backtrace to `panic.log`, then calls `opencastReleaseStayAwake`, which releases the IOKit assertion without touching AppKit. `catch_unwind` around `opencast_call` still turns one panicked op into `{"error":"internal error"}` and logs the payload. The signal path releases the assertion before exit. The kernel also drops the assertion when the process dies.
