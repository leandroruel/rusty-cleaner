# Rusty Cleaner

<p align="center">
  <img src="assets/app-icon.png" alt="Rusty Cleaner" width="128" height="128" />
</p>

Rusty Cleaner is an open-source file-cleaning application written in Rust, with a desktop interface for Linux, macOS, and Windows. It helps you find and safely review files taking up disk space — and move them to the system trash after your confirmation.

**[Download the latest release](https://github.com/leandroruel/rusty-cleaner/releases/latest)** — installers for Linux (.deb, .rpm, .AppImage), macOS (.dmg) and Windows (.exe, .msi).

## Project vision

Rusty Cleaner aims to help users find and safely review:

- Leftover data from uninstalled applications
- Temporary files and application caches
- Media and downloads from messaging apps such as WhatsApp, Telegram, and Discord
- Files in the system trash or recycle bin
- Browser caches and other browser data
- Exact duplicate files
- Large files that have not been modified for a long time

The desktop app provides a dashboard with live scan progress, per-category review, and safeguards against deleting important data.

## Project vision

Rusty Cleaner aims to help users find and safely review:

- Leftover data from uninstalled applications
- Temporary files and application caches
- Media and downloads from messaging apps such as WhatsApp, Telegram, and Discord
- Files in the system trash or recycle bin
- Browser caches and other browser data
- Exact duplicate files
- Large files that have not been modified for a long time

The long-term product is intended to provide a desktop interface, clear explanations for every finding, user-controlled selection, and safeguards against deleting important data. These are project goals, not claims about the current prototype.

## Features

The desktop app scans for these candidates and lets you review, select, and move them to the system trash:

| Feature | What it finds |
| --- | --- |
| `orphan` | Application data left behind by uninstalled programs, cross-referenced with installed package databases (pacman/dpkg/flatpak on Linux, Homebrew on macOS). |
| `temp` | Files in the operating system's temporary directory unchanged for at least 7 days. |
| `chat-media` | Media in recognized Telegram, Discord, and WhatsApp directories, with totals by type (video, image, audio, other). |
| `trash` | Files in known trash locations, with restore and empty actions. |
| `browser` | Files under recognized browser cache paths, broken down by browser (Chrome, Firefox, Edge, Brave, Safari, Chromium). |
| `duplicates` | Files of at least 1 MiB, grouped by size and hash, then verified byte by byte and grouped in the results table. |
| `large-old` | Files of at least 100 MiB in the home directory unchanged for at least 180 days. |

The interface shows live scan progress (current folder, elapsed time), a real-time system monitor (CPU, memory, disk), and supports English and Portuguese. Scans skip package-manager and toolchain directories. Cleaning always requires explicit confirmation and only moves files to the system trash — nothing is deleted permanently.

## Requirements

- Rust 1.70 or later
- Cargo

## Build and run

```sh
cargo build
cargo run -- --help
cargo run -- scan
cargo run -- scan --feature large-old
```

Run the desktop app in development mode with Node.js and the Tauri system prerequisites installed:

```sh
npm install
npm run tauri -- dev
```

Run the test suite with:

```sh
cargo test
```

## Activity log

Rusty Cleaner keeps an append-only activity log (JSON Lines) with successful scans, failures, files moved to the trash and trash-emptying operations. It lives in the platform data directory (`~/.local/share/rusty-cleaner/activity.log` on Linux, `~/Library/Application Support/rusty-cleaner/activity.log` on macOS, `%APPDATA%\rusty-cleaner\activity.log` on Windows). Logging failures never interrupt a scan.

## Safety

- Cleaning only moves the files you explicitly select to the system trash, after a confirmation dialog. Nothing is deleted permanently, and items can be restored from the trash.
- Review every result before deciding whether it is safe to remove. Age, size, and location do not prove that a file is unnecessary.
- Leftover application data detection is heuristic and may include data still used by installed software.

## Contributing

Rusty Cleaner is an open-source project, and contributions are welcome. Bug reports, platform-specific testing, documentation improvements, and feature proposals can be submitted through GitHub issues and pull requests.

Code style is enforced in CI: `rustfmt` for formatting and Clippy with warnings denied for linting. Scanners follow the Strategy pattern — see [CONTRIBUTING.md](CONTRIBUTING.md) for the conventions.
