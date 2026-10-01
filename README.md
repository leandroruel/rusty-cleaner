# Rusty Cleaner

<p align="center">
  <img src="assets/app-icon.png" alt="Rusty Cleaner" width="128" height="128" />
</p>

Rusty Cleaner is an open-source file-cleaning application written in Rust, with a desktop interface for Linux, macOS, and Windows. It helps you find and safely review files taking up disk space — and move them to the system trash after your confirmation.

**[Download the latest release](https://github.com/leandroruel/rusty-cleaner/releases/latest)** — installers for Linux (.deb, .rpm, .AppImage), macOS (.dmg) and Windows (.exe, .msi).

> **Windows note**: the installer is not yet signed with a commercial certificate, so SmartScreen may show a warning. Click "More info" → "Run anyway". The release pipeline supports SignPath signing — once a free open-source certificate is configured, future releases will be signed automatically.

## Project vision

Rusty Cleaner aims to help users find and safely review:

- Leftover data from uninstalled applications
- Temporary files and application caches
- Media and downloads from messaging apps such as WhatsApp, Telegram, and Discord
- Files in the system trash or recycle bin
- Browser caches and other browser data
- Exact duplicate files
- Large files that have not been modified for a long time

## Features

| Feature | What it finds |
| --- | --- |
| `orphan` | Application data left behind by uninstalled programs, cross-referenced with installed package databases. |
| `temp` | Files in the operating system's temporary directory unchanged for at least 7 days. |
| `chat-media` | Media from Telegram, Discord, WhatsApp, Signal, Slack and Element — with a per-messenger gallery and content sniffing for hash-named blobs. |
| `trash` | Files in known trash locations, with restore and empty actions. |
| `browser` | Browser cache directories (Chrome, Firefox, Edge, Brave, Chromium) — cleaned in place, one finding per cache tree. |
| `duplicates` | Byte-identical files of at least 1 MiB, pre-filtered with a head hash before any full-file read. |
| `large-old` | Files of at least 100 MiB unchanged for at least 180 days. |
| `registry` | Windows Registry issues (15 CCleaner-style rules), with mandatory `.reg` backup and a System Protection step before any fix. |
| `applications` | Installed apps with disk size and last-used time; one-click uninstall through the platform's package manager. |
| `themes` | Community themes from external repositories — the CSS IS the theme. |

The interface shows live scan progress, a per-messenger media gallery with thumbnails, a system monitor (CPU, memory, disk), desktop notifications when a scan completes in the background, and supports English and Portuguese. Cleaning always requires explicit confirmation. Cache directories are purged in place (they are regenerable); user files go to the system trash and can be restored.

## Requirements

- Rust 1.70 or later
- Cargo
- Node.js 22+ and npm (for the desktop app)

## Build and run

```sh
cargo build
cargo run -- --help
cargo run -- scan
cargo run -- scan --feature large-old
```

Run the desktop app in development mode:

```sh
npm install
npm run tauri -- dev
```

Run the test suite:

```sh
cargo test
```

## Privacy and telemetry

Rusty Cleaner collects **anonymous crash reports** through Sentry, strictly on an **opt-in basis** (disabled by default). We comply with Brazil's LGPD.

**What we collect** (only after you enable "Send anonymous crash reports" in Settings):

| Data | Example | Personal? |
| --- | --- | --- |
| Error messages and stack traces | `scan failed: backup failed` | No |
| Which feature was running | `scan:browser` | No |
| App version and platform | `0.5.0, linux` | No |
| Timestamp of the error | `2026-09-30T23:00:00Z` | No |

**What we never collect:**
- File paths (scrubbed before sending)
- File contents or names
- Usernames, emails, or account IDs (stripped from every event)
- Browsing history or personal information
- Any data from files scanned or cleaned

**Your choices:**
- Crash reporting is **OFF by default** — nothing is sent unless you explicitly enable it
- Toggle at any time in **Settings → General → Send anonymous crash reports**
- The app works fully offline with crash reporting disabled
- Forks and self-builds without the Sentry DSN send nothing (the SDK is a no-op)

The Sentry `beforeSend` hook strips file paths and request data from every event before it leaves your machine. See `web/src/telemetry.ts` for the exact scrubbing logic.

## Activity log

Rusty Cleaner keeps a local, append-only activity log (JSON Lines) with successful scans, failures, files moved to the trash and trash-emptying operations. This log stays on your machine and is never sent anywhere. It lives in the platform data directory (`~/.local/share/rusty-cleaner/activity.log` on Linux, `~/Library/Application Support/rusty-cleaner/activity.log` on macOS, `%APPDATA%\rusty-cleaner\activity.log` on Windows).

## Safety

- Cleaning only moves the files you explicitly select to the system trash, after a confirmation dialog. Nothing is deleted permanently, and items can be restored from the trash.
- Browser and messenger cache directories are deleted in place (they are regenerable) — the dialog says so explicitly.
- Registry fixes require a mandatory `.reg` backup and guide you through creating a system restore point before any change.
- Review every result before deciding whether it is safe to remove. Age, size, and location do not prove that a file is unnecessary.
- Leftover application data detection is heuristic and may include data still used by installed software.

## Contributing

Rusty Cleaner is an open-source project, and contributions are welcome. Bug reports, platform-specific testing, documentation improvements, and feature proposals can be submitted through GitHub issues and pull requests.

Code style is enforced in CI: `rustfmt` for formatting and Clippy with warnings denied for linting. Scanners follow the Strategy pattern — see [CONTRIBUTING.md](CONTRIBUTING.md) for the conventions.

## License

MIT
