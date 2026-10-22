# AGENTS.md

## Rules for emulator changes

These rules come from bugs that reached trunk. They apply to every agent and environment.

1. **Run the full test suite.** Run `cargo test` with `test_app` before you open a PR. TestApp needs LLVM 12 in `tests/llvm` and the common-3.0 SDK in `tests/common-3.0.sdk`; the macOS job in `.github/workflows/HyperHLE_release.yml` shows where to get them. If you cannot run TestApp, write "TestApp not run" in the PR. Never delete, skip or weaken a failing test or CI step to get a green run: fix the cause, or leave it failing and report it.
2. **Do not change shared behavior for one game.** A change that affects every app must match real iOS. If one game needs different behavior, add a general option in `src/options.rs` and enable it for that bundle ID in `touchHLE_default_options.txt`. Do not add new `bundle_identifier()` checks in code.
3. **When a game crashes, find the cause instead of resuming it.** This covers `abort`, an unexpected `exit`, uncaught exceptions, invalid instructions and bad memory accesses.
   - Do not add code that continues after the error: skipping the instruction, returning zero, unwinding the stack or returning to a caller. The session must end as it would on iOS.
   - Find what the emulator did wrong before the error. Start with the last emulated API calls in the log and check their results against Apple's documentation and touchHLE. The cause can also be in the loader, CPU, memory or graphics code, or in the game itself.
   - If the emulator is wrong, fix it and add a test that fails without the fix: a TestApp test where possible, otherwise a Rust test.
   - If you cannot find the cause, leave the game crashing and report the log, the last API calls and what you checked. A crash with a clear log is easier to fix than a game that continues with corrupted state.
   - If iOS itself lets the app continue (a signal handler, `@try`/`@catch`), implement that iOS mechanism instead.
4. **Match iOS and cite the source.** Before you implement or change an API, check Apple's documentation (old references are on web.archive.org) and touchHLE. Link the source in the commit message or the test.
5. **Search before adding.** Before you add a class, method, function or constant, search for an existing one. The `no_duplicate_*` tests fail on duplicates.

## Cursor Cloud specific instructions

### Overview

This repo contains two products:
1. **touchHLE emulator** (Rust) — an HLE iPhone OS app emulator
2. **AppDB** (Python/FastAPI) — a compatibility report web app at `appdb/`

### Building the Rust emulator

- **Git submodules must be initialized** before building: `git submodule update --init`
- The build requires `libboost-dev`, `libstdc++-14-dev`, `libsndio-dev`, and X11 development headers (`libxext-dev`, `libx11-dev`, etc.) on Linux.
- On this Cloud Agent VM (x86_64 Linux with clang), you **must** set `RUSTFLAGS="-C link-arg=-latomic"` for linking to succeed. OpenAL Soft's static build uses `std::atomic` operations that require `libatomic`, but the build scripts don't link it automatically.
- Build: `RUSTFLAGS="-C link-arg=-latomic" cargo build`
- The emulator binary requires a display/window system (SDL2) to run graphically. In headless environments, use `--help`, `--copyright`, or `--info` flags to verify the binary works.

### Linting and testing

- Lint: `dev-scripts/lint.sh` (runs comment-length check + `cargo clippy -- --deny warnings` + `cargo doc`). Note: the `t` branch has pre-existing long comment lines and clippy warnings with Rust 1.95+, so the full lint script will fail. Running `cargo clippy` alone (without `--deny warnings`) confirms no hard errors.
- Tests: `RUSTFLAGS="-C link-arg=-latomic" cargo test -- --skip test_app` (the `test_app` integration tests require a custom LLVM/SDK toolchain not available in the Cloud VM). This is not a full check: say in the PR that TestApp was not run. CI runs it on macOS.
- Format: `dev-scripts/format.sh` (requires `clang-format` for C/C++ files).

### Running the AppDB

- Setup: `cd appdb && python3 -m venv .venv && source .venv/bin/activate && pip install -e .`
- Run: `cd appdb && source .venv/bin/activate && uvicorn app.main:app --host 0.0.0.0 --port 8000 --reload`
- The SQLite DB is auto-created and seeded with demo data on first startup.
- Submitting reports requires GitHub OAuth (`GITHUB_OAUTH_CLIENT_ID` / `GITHUB_OAUTH_CLIENT_SECRET` env vars); without these, the app works in read-only mode.
- The triage JSON API requires `APPDB_TRIAGE_TOKEN` env var.
