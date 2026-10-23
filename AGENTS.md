# For agents working in this repository

Player Two is a chat for a small team and the coding agents on their own Windows machines. Someone can ask a teammate's agent for help with their Python, their proxy or their repo, and nothing runs on a machine until its owner says so.

The repository is being rebuilt from scratch. The previous version, in TypeScript and Electron, is tagged `v0.7.1`. When a task says to lift something from it, read it with `git show v0.7.1:<path>`. Before changing anything, read:

1. [`docs/plan.md`](docs/plan.md): what v1 is, what it is not, and the decisions.
2. [`docs/plan/README.md`](docs/plan/README.md): how to work through the plan.
3. The phase file you are on, and [`docs/plan/protocol.md`](docs/plan/protocol.md) when it points there.

Find the first unticked task in the lowest-numbered phase file and continue from there.

## The short version of the rules

- One task, one commit, with the task's box ticked in that commit. Work on a branch, never on `main`.
- Before every commit, from the repository root (once task P0-2 has made them exist):

  ```sh
  npm --prefix ui run check
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```

  The UI check comes first because the app crate embeds `ui/dist`. Windows CI runs the same. Tick a task that touches Windows-only code, or the app's behaviour on Windows, only after CI is green on your pushed branch.
- Rust follows [Google's Rust style guide](https://google.github.io/styleguide/rustguide.html), except its Chrome- and Fuchsia-only rules. Here that means async on tokio, plain `#[test]` and `#[tokio::test]`, and a normal Cargo workspace. Use `anyhow` for errors in the two binaries. `unsafe` goes only in `crates/win`, and every block gets a `SAFETY:` comment.
- The UI is strict TypeScript: no `any`, no `as` casts, no `!`, no `@ts-ignore`. Its types for talking to the desk are generated from Rust. Never write them by hand.
- Do not weaken `rustfmt.toml`, the clippy command, `ui/tsconfig.json`, or the ESLint or Prettier configuration.
- Nothing runs on a machine unless its owner said so.
- The window draws text written by other people and their agents, and must never execute it:
  - no raw HTML;
  - no remote images;
  - the content security policy stays;
  - the webview gets no Tauri plugin that touches the disk, the shell or the network.
- This repository is public. Never commit an internal hostname, a token, a company name, or anything else from a company network. Placeholders say `TODO`.
- Build only what the plan lists. Anything else goes under "Not decided" in `docs/plan.md` and waits for the owner's yes.
- Tasks marked **(workstation)** need a person on a Windows work machine. Leave them unticked and list them in your report.
- When the plan is wrong, write what you found under "Notes" at the end of the phase file, do the nearest correct thing, and say so.
- Keep docs and commit messages short. A commit subject is one plain sentence saying what changed.

## Layout

These paths appear during phase 0.

| Path | What it is |
|---|---|
| `crates/core/` | The wire types the hub and the desk share. No disk, no network, no processes. |
| `crates/hub/` | The hub: its SQLite database, its HTTP API and its admin commands. Builds `player-two-hub`. |
| `crates/desk/` | One person's side: their hub client, places, agents, approvals and runs. Everything the window does goes through it. |
| `crates/app/` | The Tauri shell: the window, tray, single instance, start at login and updates. It is thin and passes the UI's requests to the desk. |
| `crates/win/` | The only `unsafe` code: Windows calls no crate covers. Made only if a task needs it. |
| `ui/` | The React window, built with Vite, Tailwind and shadcn/ui. |
| `deploy/` | The hub's image and its Kubernetes manifests. |
| `build.yaml` | The Azure DevOps release pipeline. A stub with `TODO`s for the owner to fill in. |
| `docs/plan/` | The implementation plan. |
