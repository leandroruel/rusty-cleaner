# Contributing to Rusty Cleaner

Thanks for helping! This document describes the conventions that keep the
project consistent. CI enforces them on every push and pull request.

## Code style

- **Formatter**: `rustfmt` (config in `rustfmt.toml`). Run `cargo fmt --all`
  before committing; CI runs `cargo fmt --all -- --check`.
- **Linter**: Clippy with warnings denied. Run
  `cargo clippy --workspace --all-targets -- -D warnings` locally; CI fails on
  any warning.
- **Frontend**: TypeScript strict mode. Run `npm run typecheck` and
  `npm run build`.

## Architecture and design patterns

- **Strategy pattern for scanners**: every cleaning feature implements the
  `Scanner` trait (`src/scanner.rs`) and is registered once in
  `scanners()` (`src/lib.rs`). To add a feature, create a module with a
  `scan()` function and register it — never add `match` arms per feature.
- **Shared walker**: all filesystem traversal goes through
  `scanner::walk_files`, which skips symlinks and excluded directories.
- **Exclusions are mandatory**: never scan package-manager or toolchain
  directories (`node_modules`, `target`, `.cargo`, flatpak/snap stores, etc.).
  Extend `is_excluded_dir` in `src/scanner.rs` when a new one appears.
- **Safety first**: cleaning only moves user-selected files to the system
  trash after explicit confirmation. Never delete permanently.

## Commits

Use conventional commits (`feat:`, `fix:`, `docs:`, `refactor:`...). Keep each
commit focused and validated with the commands above.
