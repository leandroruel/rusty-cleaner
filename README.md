# CleanOS Pro (rusty-cleaner)

A cross-platform Rust CLI prototype for finding cleanup candidates on Linux,
macOS, and Windows. Scans are read-only: this version never deletes files.

## Build and run

```sh
cargo run -- scan
cargo run -- scan --feature large-old
```

Use `cargo run -- --help` to see supported features. Candidates should be
reviewed manually before deletion. Orphan application data is heuristic and
must not be treated as proof that a file is safe to remove.

## Initial scanners

- Orphan/stale application data candidates
- Temporary files
- Chat media (WhatsApp, Telegram, Discord)
- Trash/recycle-bin contents
- Browser cache and profile data candidates
- Duplicate files (same size and content hash)
- Large files not modified recently

The desktop interface and deletion workflow are not implemented yet. The
project is intentionally read-only until the scanners and platform paths have
been validated on each target operating system.
