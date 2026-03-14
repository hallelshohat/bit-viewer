# Repository Guidelines

## Project Structure & Module Organization

This repository is centered on the native Rust app in `desktop-rust/`. Source files live in `desktop-rust/src/`:

- `main.rs`: app entry point and window startup
- `app.rs`: egui UI, layout, shortcuts, and interaction logic
- `document.rs`: file loading and memory-mapped access
- `filters.rs`: stacked filter pipeline and group operations
- `viewer.rs`: row layout and rendering helpers

Build artifacts go under `desktop-rust/target/` and must not be committed.

## Build, Test, and Development Commands

Run commands from `desktop-rust/` unless noted otherwise.

- `cargo run`: start the desktop app in debug mode
- `cargo run --release`: run an optimized local build
- `cargo build`: compile and catch type or borrow-check errors
- `cargo test`: run unit tests
- `cargo fmt --all`: format the codebase
- `cargo build --release --target x86_64-pc-windows-gnu`: cross-compile a Windows binary from Linux

## Coding Style & Naming Conventions

Use standard Rust formatting with `cargo fmt`. Prefer small functions and explicit data flow over deep abstraction. Follow Rust naming conventions:

- `snake_case` for functions, files, and variables
- `CamelCase` for structs and enums
- `SCREAMING_SNAKE_CASE` for constants

Keep UI constants near the top of `app.rs`. Add brief comments only where bit layout, viewport math, or filter ordering is non-obvious.

## Testing Guidelines

Add unit tests close to the implementation using `#[cfg(test)]` modules. Focus tests on filter semantics, row packing, offset math, and boundary cases for large files. Name tests by behavior, for example `select_range_preserves_group_order`. Run `cargo test` before opening a PR.

For UI work, do not rely on code inspection alone. Run the native app and verify both visuals and behavior:

- launch with a real file using `cargo run -- /path/to/file` so the viewer opens directly into a loaded state
- check both the empty state and a loaded multi-row file
- verify pane widths, text contrast, bit-grid readability, and status/footer wrapping at the default window size
- exercise at least the main viewer shortcuts: `h`, `Home`, `End`, `Page Up`, and `Page Down`
- confirm bit, hex, and ASCII panes stay aligned while scrolling
- capture screenshots for before/after comparisons; in headless Linux environments, use `Xvfb` plus `import -window root ...` to inspect the UI visually
- still run `cargo fmt --all`, `cargo build`, and `cargo test` after UI changes

## Commit & Pull Request Guidelines

Recent history is short and inconsistent (`init commit`, `nice`, `remove fullstack:`). Prefer clear imperative commits such as `viewer: sync hex and ascii scroll`. Keep each commit focused.

PRs should include:

- a short summary of the user-visible change
- notes on any rendering, filter, or file I/O tradeoffs
- screenshots or a short recording for UI changes
- the commands you ran, such as `cargo fmt --all`, `cargo build`, and `cargo test`
