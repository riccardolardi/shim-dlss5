# shim

A small, honest DLSS 5 injector for Windows. It scans your game library, tells
you exactly what it will do to a game, does it, and can undo it. Nothing else.

Status: **Phase 0, scaffold.** The app runs, has its three screens and settings,
but does not yet scan or install anything. See [PLAN.md](PLAN.md) for scope,
constraints, and phases.

## Principles

- Reversible: every written file is recorded, every overwritten file is backed up.
- Transparent: you can read the file plan before anything happens.
- Bundle nothing: components come from their publishers, pinned by version and
  SHA-256. The DLSS 5 model is your own file; it is never downloaded.
- One job: no desktop post-processing, no community features, no telemetry.

## Stack

Tauri 2 · Rust core (`crates/core`, OS-independent, tested everywhere) ·
Windows glue (`crates/win`) · React 19 + Vite + Tailwind 4.

## Develop

```sh
npm install
npm run tauri dev        # desktop app with hot reload
npm test                 # front-end unit tests
cargo test --workspace   # Rust tests; also regenerates src/lib/generated/*.ts
```

On macOS or Linux the app builds and runs for UI work; scanning and installing
are Windows-only and say so in the UI. Logs go to `<data dir>/logs/shim.log`.

## Layout

```
crates/core/   models, settings, library, discovery, planning (no OS calls)
crates/win/    registry, Authenticode, elevation (Windows only)
src-tauri/     Tauri shell: state + commands, nothing else
src/           React app: screens, components, store, i18n, generated types
```

## Licence

MIT. Third-party components shim downloads keep their own licences.
