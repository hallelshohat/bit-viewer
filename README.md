# Bit Viewer

Bit Viewer is now a native Rust desktop application for exploring binary files through synchronized bit, hex, and ASCII views. The project is built with `eframe/egui` and runs as a single monolithic app on Linux and Windows.

## Repository layout

```text
bit-viewier/
├── desktop-rust/
│   ├── src/
│   │   ├── app.rs
│   │   ├── document.rs
│   │   ├── filters.rs
│   │   ├── main.rs
│   │   └── viewer.rs
│   ├── Cargo.toml
│   ├── Cargo.lock
│   ├── .gitignore
│   └── README.md
├── .gitignore
└── README.md
```

## Features

- Native desktop UI with no web frontend or backend server.
- Direct local file access through memory mapping.
- High-performance bit grid with configurable row width and bit size.
- Separate, resizable bit / hex / ASCII panes.
- Jump to byte or bit offset.
- Ordered filter pipeline with stacked operations.
- Group-aware filters, including preamble sync, group length selection, and per-group bit-range extraction.
- Keyboard shortcuts for navigation and view controls.
- Linux and Windows builds from the same Rust codebase.

## Run locally

Install Rust with `rustup`, then:

```bash
cd desktop-rust
cargo run
```

For an optimized build:

```bash
cd desktop-rust
cargo run --release
```

## Build targets

- Linux: `cargo run --release`
- Windows from Windows: `cargo build --release`
- Windows cross-compile from Linux:

```bash
cd desktop-rust
cargo build --release --target x86_64-pc-windows-gnu
```

## Main design decisions

- The file is opened locally and memory-mapped instead of being uploaded into a service.
- The bit grid renders only the visible viewport and uses a cached texture path to keep scroll performance acceptable on large files.
- Filters are applied as an ordered pipeline so transforms and group operations can be stacked predictably.
- The UI uses native split panes and keeps the viewer in one process, which removes serialization, HTTP, and browser overhead.

## More detail

See [desktop-rust/README.md](/home/hallel/code/bit-viewier/desktop-rust/README.md) for desktop-specific notes.
