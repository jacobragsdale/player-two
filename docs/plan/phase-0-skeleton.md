# Phase 0: Skeleton

Every part exists, does nothing yet, and is checked on every push. Read [`../plan.md`](../plan.md) first.

## Tasks

- [x] **P0-1 The Cargo workspace.**
  - Make `Cargo.toml` at the root, with the crates `crates/core` (`player-two-core`), `crates/hub` (`player-two-hub`, a binary), `crates/desk` (`player-two-desk`) and `crates/app` (`player-two-app`), each a stub.
  - Add `rust-toolchain.toml` (stable, with clippy and rustfmt) and `rustfmt.toml` (`max_width = 100`). Edition 2024.
  - The Rust checks in `AGENTS.md` pass.
- [x] **P0-2 The UI.**
  - `ui/` uses Vite, React, strict TypeScript, ESLint, Prettier, Tailwind and Vitest.
  - `npm run check` runs typecheck, lint, format check, test and build, in that order.
  - Lift the TypeScript, ESLint and Prettier settings from v0.7.1 (`tsconfig.json`, `eslint.config.mjs`, `prettier.config.mjs`), dropping what was there for Node and Electron. They must end up no weaker.
- [x] **P0-3 The look.** Lift v0.7.1's shadcn components (`src/window/renderer/components/ui/`), its theme and `components.json` into `ui/`. Keep only the components the UI will use, and add the rest when a task needs them.
- [x] **P0-4 The Tauri shell.** `crates/app` opens `ui/dist` in a window, and allows only one instance.
  - The content security policy is v0.7.1's (`default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:`), plus the IPC origins Tauri needs in `connect-src`.
  - The webview's capabilities grant only the `desk` command and the `desk` event.
  - Add no Tauri plugin the webview can call.
- [x] **P0-5 The desk boundary.** As described in [`protocol.md`](protocol.md#the-desk-boundary):
  - `crates/desk/src/ui.rs` has `Request`, `Response` and `Event`, with one `Hello` request to start.
  - A test writes `ui/src/desk.ts` with `specta-typescript`, and fails when the file is out of date.
  - The app's `desk` command passes requests to the desk, and the window shows the desk's hello.
- [x] **P0-6 Windows CI.** In `.github/workflows/ci.yml`:
  - Remove the `hashFiles` guards.
  - Build the NSIS installer with `cargo tauri build --debug` and upload it as an artifact.
- [ ] **P0-7 (workstation)** Download the CI installer and install it on a work machine without admin rights. The window opens and shows the desk's hello. Note what SmartScreen says.

## Done when

Every check is green here and on Windows CI, and a work machine can install the debug build and open the window.

## Notes

- **P0-4: the app crate builds on Linux.** The plan assumed this machine could not build it. It can, against WebKitGTK, tray included (the tray's library is loaded at run time). So every check now covers the whole workspace, with the UI built first, since the app embeds `ui/dist`.
