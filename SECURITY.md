# Security policy

## What OpenCast is

A local launcher. It runs commands, scripts and MCP servers **as your user, with
your permissions** — that is the product. Anything you put in `config.toml`
(scripts, `[[commands]]` commands, `api_key_cmd`, MCP server command lines) is
code you asked it to run. There is no sandbox: a launcher that could not run
`brew` would be useless.

So the threat model is: your config folder, your Keychain items OpenCast wrote,
and the network endpoints you configure.

## What is not collected

- No telemetry, no analytics, no crash reporting.
- No network traffic you did not configure. The two opt-in fetches are
  `[general] check_updates` (GitHub releases for `update_repo`) and
  `[general] currency` (daily exchange rates); both default to off.
- AI prompts go only to the providers in your `[[providers]]` tables. Local
  providers (Ollama, LM Studio) keep everything on the machine.
- Clipboard history, pins, notes, chats and usage live in
  `~/.config/opencast/` and are only read by the app. “Sync Config” symlinks
  those files into iCloud Drive or a folder you set (`sync_dir`) — that is your
  choice, and it is visible in Settings.

## Where secrets live

API keys are stored in the macOS Keychain (generic passwords, service names
`opencast-<provider-id>`). The config file stores only the *command* that fetches
a key (`api_key_cmd`, e.g. `security find-generic-password -s opencast-xai -w`)
or an environment variable name (`api_key_env`). Keys are never written to
`config.toml`, to sync files, or to logs.

The Keychain items are readable by `/usr/bin/security` without a prompt — that
is what makes `api_key_cmd` work. Anything that can run as you can already read
your Keychain with your login password, so this adds no remote exposure, but it
does mean a process running as you can read these keys without a dialog.

## Reporting a vulnerability

Open a private report from the **Security** tab of the repository (“Report a
vulnerability”), or email the maintainer address on the repo profile. Please do
not open a public issue for anything exploitable. Expect a reply within a week;
fixed reports get a note in `CHANGELOG.md` and credit if you want it.

Things worth reporting: anything where a *config file you did not write* or a
*page or file you did not open* ends up executing, any secret leaving the
machine, any path where the panel writes outside its config folder.
