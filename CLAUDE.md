# Wordle Solver

Interactive Wordle solver with CLI and HTMX web interface. Scrapes past answers to exclude used words, then narrows candidates using positional feedback (green/yellow/gray).

## Build & Test

```bash
cargo build                                                     # Build everything
cargo test                                                      # Run all tests
cargo clippy --all-features --all-targets                       # Check code quality
cargo clippy --all-features --all-targets --allow-dirty --fix   # Auto-fix clippy warnings before fixing manually
cargo fmt                                                       # Format code
cargo run                                                       # Run CLI solver
cargo run --bin web                                              # Run web interface (port 3000)
docker build -t wordle-solver .                                  # Build Docker image (web only)
```

- All tests must pass before committing
- `cargo clippy` must pass with no warnings
- `cargo fmt` must pass with no formatting changes

## Commit Style

- Prefix: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`
- Summary line under 80 chars
- Summarize what was added/changed and why

## Architecture

- **Single crate** (`wordle_word`) with a library and two binaries
- `src/lib.rs` — core logic: word fetching, candidate filtering, letter-frequency scoring
- `src/main.rs` — CLI binary: interactive guess/feedback loop in the terminal
- `src/bin/web.rs` — Web binary: Axum + Askama + HTMX, session-based game state
- `templates/` — Askama HTML templates (`base.html`, `game.html`, `partials/`)
- Word list scraped at runtime from rockpapershotgun.com (past Wordle answers excluded)
- Web state: `Arc<RwLock<HashMap<Uuid, Session>>>` keyed by session UUID

## Conventions

- Library (`lib.rs`) exposes pure functions — no I/O beyond the HTTP word-list fetch
- Web uses HTMX for partial page updates, no client-side JS framework
- Askama templates with `WebTemplate` derive for Axum integration
- Docker image uses cargo-chef for cached dependency builds

## Issue Tracking (beads)

This project uses `br` (beads_rust) for local issue tracking. Issues live in `.beads/` and are committed to git.

```bash
br list                                        # Show all open issues
br list --status open --priority 0-1 --json    # High-priority open issues (machine-readable)
br ready --json                                # Actionable issues (not blocked, not deferred)
br show <id>                                   # Show issue details
br create "Title" -p 2 --type feature          # Create an issue (types: feature, bug, task, chore)
br update <id> --status in_progress            # Claim work
br close <id> --reason "explanation"           # Close with reason
br dep add <id> <depends-on-id>                # Express dependency
br sync --flush-only                           # Export to JSONL for git commit
```

- **Priority scale**: 0 = critical, 1 = high, 2 = medium, 3 = low, 4 = backlog
- **Statuses**: `open`, `in_progress`, `deferred`, `closed`
- **Labels**: use to categorize by area (`core`, `cli`, `web`, etc.)
- Use `RUST_LOG=error` prefix when parsing `--json` output to suppress log noise
- `br` never auto-commits — run `br sync --flush-only` then commit `.beads/` manually
- Check `br ready --json` at the start of a session to see what's actionable
- Close issues with descriptive `--reason` so context is preserved
