# Rusty Cleaner

Rusty Cleaner is an open-source file-cleaning application written in Rust. The goal is to grow it into a complete, cross-platform cleaner for Linux, macOS, and Windows, with a broad set of tools to help people understand and manage files taking up disk space.

The project is under active development. The current version is an experimental command-line prototype. Scans are read-only: **no files are deleted**.

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

## Current status

The current CLI and desktop prototype can scan for these candidates:

| Feature | Current behavior |
| --- | --- |
| `orphan` | Lists old files in selected application-data directories. Age is only a heuristic; it cannot confirm that an application was uninstalled. |
| `temp` | Lists files in the operating system's temporary directory that have not changed in at least 7 days. |
| `chat-media` | Looks for media in recognized Telegram and Discord directories. WhatsApp does not yet have a dedicated location scanner. |
| `trash` | Lists files in known trash locations. |
| `browser` | Looks for files under recognized browser cache paths. |
| `duplicates` | Finds files of at least 1 MiB, groups by size and hash, then verifies matching content byte by byte. |
| `large-old` | Lists files of at least 100 MiB in the home directory that have not changed in at least 180 days. |

Platform paths and scanners are early implementations and have not yet been validated across all supported operating systems. The desktop interface is an early prototype: it scans, lets you review and select findings, and can move selected files to the system trash after an explicit confirmation. It never deletes files permanently.

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

## Safety

- Cleaning only moves the files you explicitly select to the system trash, after a confirmation dialog. Nothing is deleted permanently, and items can be restored from the trash.
- Review every result before deciding whether it is safe to remove. Age, size, and location do not prove that a file is unnecessary.
- Leftover application data detection is heuristic and may include data still used by installed software.

## Contributing

Rusty Cleaner is an open-source project, and contributions are welcome. Bug reports, platform-specific testing, documentation improvements, and feature proposals can be submitted through GitHub issues and pull requests.

Code style is enforced in CI: `rustfmt` for formatting and Clippy with warnings denied for linting. Scanners follow the Strategy pattern — see [CONTRIBUTING.md](CONTRIBUTING.md) for the conventions.
