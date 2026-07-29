# IRIS

A small non-destructive PNG image editor, built in Rust with [egui](https://github.com/emilk/egui)/eframe.

Edits are kept as an ordered stack of objects (crop, pasted images, ...) applied on top of a base image, rather than baked into pixels — so they can be re-ordered, hidden, or adjusted after the fact.

## Features

- Open / Save PNG (`Ctrl+O` / `Ctrl+S`)
- Non-destructive crop via drag-to-crop, with a live overlay
- Copy / Cut / Paste through the system clipboard (`Ctrl+C` / `Ctrl+X` / `Ctrl+V`) — acts on the selected image object if one is selected, otherwise the full composite
- Select, drag-to-move, and delete objects (`Delete` / `Backspace`) directly on the canvas
- Object stack panel with visibility toggles, and an inspector for numeric editing

## Running

```sh
cargo run
```

Requires a recent stable Rust toolchain (edition 2024).

## Project layout

| File | Responsibility |
|---|---|
| `src/main.rs` | Window setup / entry point |
| `src/app.rs` | UI, tools, and interaction state (`eframe::App` impl) |
| `src/document.rs` | Serializable document model |
| `src/object.rs` | Object stack kinds (`Crop`, `Image`) |
| `src/render.rs` | Compositing the object stack into a pixmap |
| `src/plugin.rs` | Stub extension point for future object kinds |

## Status

Early and actively developed — no undo/redo, multi-select, or resize/rotate handles yet.
