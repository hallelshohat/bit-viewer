# Bit Viewer Desktop

`desktop-rust/` is a native monolithic rewrite of Bit Viewer. It replaces the web frontend/backend split with a single Rust desktop application for Linux and Windows.

## Current scope

- Native file picker.
- Non-blocking file picker so the UI thread stays responsive.
- Manual path entry and drag-and-drop fallback for systems where the native chooser is slow or broken.
- Direct file access through memory mapping instead of HTTP uploads and chunk APIs.
- Virtualized row rendering with synchronized bit, hex, and ASCII views.
- Configurable row width and bit size.
- Jump to byte/bit offset.
- Basic byte-level filters:
  - invert bits
  - reverse bits inside each byte
  - XOR mask

## Why this is faster

- The file is memory-mapped locally, so the OS pages data in on demand instead of routing every viewport through a web stack.
- There is no server process, serialization, REST polling, or browser render pipeline.
- Only the visible rows are built each frame.
- The bit pane is painted directly with native `egui` drawing calls.

## Build

Install Rust with `rustup`, then:

```bash
cd desktop-rust
cargo run
```

For a release build:

```bash
cd desktop-rust
cargo run --release
```

## Platform notes

- Linux: `cargo run --release`
- Windows: `cargo run --release` or `cargo build --release`

The app is written in Rust with `eframe/egui`, so it stays a single native codebase across both platforms.
