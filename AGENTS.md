# AGENTS.md

Two products in this tree:

1. **touchHLE** (Rust) — an HLE iPhone OS app emulator. The main one: a Rust host
   reimplements iOS frameworks and runs guest ARM code (Dynarmic JIT, with an
   interpreter fallback).
2. **AppDB** (Python/FastAPI) — a compatibility report web app at `appdb/`.

Also present: `telegram_bot/` (app-fix request bot), `android/` (Gradle app),
`platform/ios/` (Applesauce host app).

## Rules for emulator changes

These come from bugs that reached trunk. They apply to every agent and environment.

1. **Run the test suite.** `cargo test` covers the unit tests and TestApp. If you
   cannot run TestApp, write "TestApp not run" and why. Never delete, skip or
   weaken a failing test or CI step: fix the cause, or leave it failing and report
   it.
2. **Do not change shared behavior for one game.** Anything affecting every app
   must match real iOS. Per-game differences go in a general option in
   `src/options.rs`, enabled for that bundle ID in `touchHLE_default_options.txt`;
   no new `bundle_identifier()` checks in code.
3. **Find the root cause before any fallback.** Check the last emulated API calls,
   loader, CPU, memory, audio and graphics state against Apple's documentation
   first. Fix the subsystem and add a regression test (TestApp, else a Rust unit
   test). `--headless` and TestApp stay strict: never mask guest `exit`, `abort`,
   uncaught exceptions or fatal signals. GUI fallbacks (legacy DRM/anti-tamper
   `exit()`/`abort()`, unhandled `SIGPIPE` from old ad SDKs) stay scoped to
   non-headless sessions and never replace a real fix.
4. **Match iOS and cite the source.** Check Apple's documentation (old references
   are on web.archive.org) and touchHLE before implementing or changing an API;
   link the source in the commit message or test.
5. **Search before adding.** Check for an existing class, method, function or
   constant first; the `no_duplicate_*` tests fail on duplicates.

## Where things live

- `src/lib.rs` — `main()` and the module list; `src/environment.rs` —
  `Environment`, threads, run loop.
- `src/environment/app_picker.rs` (~8.8k lines) — native app picker and the whole
  Settings screen; most settings/UI work lands here, not in `frameworks/`.
- `src/frameworks/<framework>/` — HLE implementations (`uikit/`, `foundation/`,
  `core_animation/`, `opengles/`, ...); `src/libc/` and `src/dyld/` — C runtime
  and loader; `src/objc/` — Objective-C runtime and the `msg!`, `msg_super!`,
  `msg_class!`, `objc_classes!` macros; `src/window.rs` — SDL window, input,
  device profiles; `src/quick_menu.rs`, `src/trainer_ui.rs` — in-game overlays,
  drawn last in `src/gles/present.rs`, fed touches before the guest in
  `src/window.rs`.
- Guest pointers are 32-bit, host pointers 64-bit: use `Ptr`/`MutPtr` from
  `src/mem.rs`, never raw host pointers, for anything the guest can see.
- `appdb/`: `cd appdb && pip install -e . && uvicorn app.main:app`.

## Adding an iOS API

- Implement in the framework module: classes in `objc_classes!` as
  `@implementation Class: Superclass`, C functions with `export_c_func!`,
  constants with `HostConstant`.
- Export via the module's `FUNCTIONS`/`CLASSES`/`CONSTANTS`, wired through its
  `DYLIB` const and listed in `DYLIB_LIST` (`src/dyld/dylib_list.rs`); nothing
  resolves until then. `no_duplicate_classes`/`_functions`/`_constants` fail on
  duplicates.
- `objc_classes!` needs all `+` class methods before the first `-` instance
  method.
- `msg!` arguments must each be a single token tree: bind complex expressions
  first (`let hidden = !selected;` then `msg![env; this setHidden:hidden]`).
  `msg_super!` only works inside `objc_classes!`.
- In `-dealloc`, `std::mem::take` on a host object resets the embedded superclass
  host object; copy `superclass` back before `msg_super![env; this dealloc]`
  (pattern at `src/frameworks/uikit/ui_view/ui_scroll_view/ui_text_view.rs:158`)
  or the superclass's retained fields leak.

## Options and the Settings screen

- Options live in `src/options.rs` (fields, parsing, a large `#[cfg(test)]` suite).
  Help text is `OPTIONS_HELP.txt`, `include_str!`'d, so `--help` and the on-disk
  copy change together.
- A new setting touches three places: `src/options.rs`, `OPTIONS_HELP.txt`, and a
  row in the Settings screen in `src/environment/app_picker.rs`.
- Per-app defaults go in `touchHLE_default_options.txt`, one line per bundle ID;
  read its header before editing.

## Build, test, lint

- `cargo`/`rustc` may be off `PATH`:
  `export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"`.
- Submodules first: `git submodule update --init` (`vendor/SDL`, `dynarmic`,
  `openal-soft`, `libxml2`, `stb`).
- Fresh builds need `export CMAKE_POLICY_VERSION_MINIMUM=3.5`: CMake 4.4 rejects
  `sdl2-sys`'s old `cmake_minimum_required`.
- `cargo test` — 652 tests in 5 suites (649 unit, 2 in `tests/integration.rs`,
  1 trainer atlas), 12 ignored doc-tests, ~25 s warm. TestApp needs `tests/llvm`
  (LLVM 12; its `clang` is x86_64, so Rosetta on Apple Silicon) and
  `tests/common-3.0.sdk`.
- One test: `cargo test --lib <name>`. TestApp only:
  `cargo test --test integration test_app`.
- Run an app: `cargo run --release -- path/to/app.app`, with the bundle in
  `touchHLE_apps/`. That directory, `touchHLE_sandbox/` and the `touchHLE_*.txt`
  runtime files are gitignored.
- Android: `android/` builds the core with `--no-default-features` and
  `sdl2/bundled` plus two static wrappers, not the default feature set (see
  `extraCargoBuildArguments` in `android/app/build.gradle`).

## Formatting and lint

Never run `cargo fmt`: `cargo fmt --check` reports 986 files because the tree is
hand-formatted (4-space indent, wrapped by hand), so it buries a real change in an
unreviewable diff. Follow the surrounding style.

`dev-scripts/lint.sh` cannot pass: 351 pre-existing comment lines over 80 columns
and ~1000 pre-existing clippy warnings that `--deny warnings` turns into a failure.
Use it as a delta check only: keep new comments within 80 columns, add no new
warnings. `cargo clippy` needs the `PATH` above.

## Commits and docs

- Subject style is a lowercase area prefix: `uikit:`, `renderer:`, `settings:`,
  `libc:`, `opengles:`.
- A user-visible change updates `README.md`, `CHANGELOG.md` and
  `OPTIONS_HELP.txt` where relevant.
- Commit messages and tests cite the iOS behaviour they match (rule 4 above).

## Optional agent tooling

Use a line only if that tool is installed; otherwise ignore it. Nothing here is
required to work on the repo.

- **Code graph (graphify)** — if `graphify-out/graph.json` exists, run
  `graphify query "<question>"` before grep or file reads, and pass that rule to
  subagents; `graphify path A B` for relationships, `graphify explain X` for one
  concept. After code changes run `graphify update .` (AST-only, no API cost);
  never hand-edit `graphify-out/`.
- **Symbol navigation (serena)** — prefer `find_symbol`,
  `find_referencing_symbols` and `replace_symbol_body` over text search for named
  symbols, especially in large files; call `initial_instructions` first.
- **Library docs (context7)** — for a question about a library, framework, SDK,
  API or CLI (SDL2, Dynarmic, a Rust crate, Gradle, FastAPI), fetch current docs
  with `resolve-library-id` then `query-docs` instead of trusting training data.
  Skip it for refactoring or debugging that does not depend on a library API.
- **Memory (mnemon, claudemem, hindsight, openviking)** — recall before answering
  anything that may depend on past sessions, decisions or preferences. On a large
  plan or exploration, record findings, decisions and the plan itself as you go,
  not only at the end: that is what survives context compaction and session
  switches. Never store secrets or transient state.
- **Context compression (caveman)** — compress a large tool output or file before
  it enters context, keep the `ccr_` handle, and retrieve when details are needed.
  Compress by need, not routinely.
- **Context management (billion-context / ACP)** — in long sessions, history is
  summarized automatically. `acp_status` shows usage and compressible ranges,
  `search_context` finds an old block by keyword, `decompress` restores it. A
  summary is not a plan of record: anything a later session must act on belongs
  in memory, not only in a compressed block.
- **Ponytail (lite/full/ultra)** — smallest complete change: reuse what exists, no
  speculative abstraction, finish the callers, tests and fixtures it needs. The
  level changes verbosity, not correctness.
- **Skills (grilling, grill-me, ...)** — when a request is ambiguous or a plan is
  expensive to get wrong, load the skill and question the user before
  implementing; skip for small, well-specified fixes.
