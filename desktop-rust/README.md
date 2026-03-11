# Bit Viewer Desktop

`desktop-rust/` is the main Bit Viewer application. It is a single Rust desktop binary for Linux and Windows, built with `eframe/egui`.

## Current scope

- Native file picker.
- Non-blocking file picker so the UI thread stays responsive.
- Manual path entry and drag-and-drop fallback for systems where the native chooser is slow or broken.
- Direct file access through memory mapping.
- Virtualized row rendering with synchronized bit, hex, and ASCII views.
- Configurable row width and bit size.
- Jump to byte/bit offset.
- Ordered stacked filters, including:
  - invert bits
  - reverse bits inside each byte
  - XOR mask
  - preamble sync
  - group length filtering
  - bit-range selection from each group

## Why this is faster

- The file is memory-mapped locally, so the OS pages data in on demand.
- There is no service boundary, serialization, or browser render pipeline.
- Only the visible rows are built each frame.
- The bit pane is painted through a cached native rendering path.

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
- Windows cross-compile from Linux: `cargo build --release --target x86_64-pc-windows-gnu`

The app stays a single native Rust codebase across both platforms.
