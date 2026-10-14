# Retoucan

A free, local photo-editing assistant for Lightroom Classic. Retoucan learns how you edit from your own Lightroom catalogs, then gives a new shoot a first pass in your style: per-photo exposure, white balance and tone, a look from any preset or LUT you choose, and AI masks for teeth, eyes, skin, subject and sky. Everything runs on your computer. Your photos never leave it.

Status: early development.

## Planned

- Learn a style from an edited Lightroom catalog (no uploads, trains in about a minute)
- Live preview of looks from `.cube` LUTs and Lightroom `.xmp` presets
- A starter set of openly licensed LUTs
- A companion Lightroom Classic plugin that applies the edits, so every slider and mask stays editable
- Installers for macOS and Windows

## Your presets stay yours

Retoucan uses the presets and looks already installed in your Lightroom, reading them from your computer when it needs them. It never copies, uploads or redistributes them. The only looks bundled with Retoucan are the eight in `looks/`, which were written for this project and are MIT licensed like the rest of it.

## Building from source

Needs [Rust](https://rustup.rs) and [Bun](https://bun.sh).

```
bun install
bun run tauri dev
```

## License

MIT
