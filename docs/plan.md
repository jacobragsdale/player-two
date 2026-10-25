# Plan

Agreed on 2026-10-22. This page says what Player Two is, what v1 holds and not, and the decisions behind it. The work, task by task, is in [`plan/`](plan/README.md).

## What it is

A chat for a small team and the coding agents on their own Windows machines. It is for two things:

- **Help with someone's machine.** A developer asks a less technical colleague's agent to fix their Python, their proxy settings or an install, and watches it work.
- **Working on a project together.** You drive your own agent in a channel while teammates watch, comment and pick up from it. You can also ask a teammate's agent to work in their checkout.

It runs inside one company network, among people who trust each other. The guardrails are there to catch mistakes, not to stop attackers.

## The rule every phase keeps

Nothing runs on a machine unless its owner said so, where they can watch it and stop it.

## v1

**People and chats**
- Channels open to everyone on the hub, and direct chats between two people. Threads under a message.
- Joining with a join code that an admin sends: it holds the hub's address and the join token. The name is prefilled from the Windows user name.
- Presence, unread counts, notifications, a tray icon, and starting at login. A comment in a thread notifies the people following it.

**Agents**
- `@bob` is Bob's machine. `@bob/name` is one of the folders he chose to share, picked with the Windows folder dialog.
- Agents speak ACP. Cursor (`agent acp`) comes first. Copilot (`copilot --acp --stdio`) and Claude Code (through its ACP adapter) are alternatives.
- Setting up an agent is one step after joining. Cursor installs in one click, Copilot and Claude Code have steps to copy, and signing in is a button.
- The desk finds agents installed on Windows and in each WSL distro. Only people who have a WSL distro, not counting Docker Desktop's, ever see WSL mentioned.
- A run may do anything its owner's user can. The agent starts in its own mode that does not ask, and the desk says yes to anything that still asks.

**Runs**
- Your own tag, sent from your own window, runs at once.
- Anyone else's tag waits for your approval card. On it you choose where it runs (Windows or a WSL distro, shown only when both apply), and whether the asker may keep going. The card warns that the agent changes files and runs commands without asking, and that everything it reads or prints is shown in the chat.
- Only a tag reaches an agent. A request is a thread with one agent session for each place it tags. A follow-up tags the place again in the thread, and needs your click unless you let the asker keep going. That lasts until you press Stop, or an hour after the agent last finished in the thread. A message without tags is a comment.
- The asker can press Stop too.
- Each place runs one thing at a time.
- Nothing runs elevated. UAC and `sudo` prompts stay yours to answer.
- Edit requests run in your actual folder. For a git checkout, the card warns when it has uncommitted changes.
- Passwords in URLs, and `password=`, `token=` and `Authorization:` values, are masked before a run's activity reaches the chat.

**Hub**
- One container, with its data in a Postgres database in the same cluster. Messages and runs are kept for 30 days, and a thread goes whole once its newest event is that old. Backups are the database instance's.
- Admin commands: `join-code`, `reset NAME` and `remove NAME`.

**Shipping**
- An unsigned, per-user Windows installer that the hub serves, along with updates. The hub and the app share one version number.
- Releases are built by an Azure DevOps pipeline, [`build.yaml`](../build.yaml).

## Decisions

| Decision | Choice | Why |
|---|---|---|
| Starting point | Wipe v0.7.1 in place and rebuild. Its code is under the tag `v0.7.1`, to lift from. | v0.7.1 was built on signed events, agent = repo, and a desk inside WSL. All three are reversed here. |
| Platform | Windows only. Linux is for development and tests. | Everyone is on Windows. |
| App shell | Tauri, on the WebView2 already on every machine. | Small installer, fast start, light in the tray. |
| Languages | Rust for the hub, the desk and the shell. TypeScript and React for the window. | The wire format is written once, in Rust. Rust handles Windows processes well. |
| Desk | A library crate that runs inside the app. The window reaches it through one Tauri command and one event. The types are generated with `specta`. | One process on the desktop. Later, a headless desk is a thin `main` over the same crate. |
| Hub | Rust, with axum and the `postgres` crate. One pod, one replica. | Small image, and it shares the wire types with the desk. The stream and presence live in the hub's memory. |
| Storage | A Postgres database of its own in the cluster, reached by a plain URL from the secret. One connection; the hub exits when it loses it. | No NFS volumes, and no SQLite file locking over NFS. One connection keeps event ids in the order they were written, which the stream relies on. |
| Trust | The hub is trusted. Everyone has a personal token from joining. No signatures. | High-trust internal network. The owner's click is the guardrail, not cryptography. |
| Agents | ACP only, through the `agent-client-protocol` crate. | Cursor, Copilot and Claude Code all speak it. |
| Runs | No modes. A request its owner approved may do anything, and asks nobody about each step. | The owner's click is the guardrail. Approving each step was friction, and an agent's own settings could skip it anyway. |
| Windows and WSL | The desk runs on Windows and starts WSL agents with `wsl.exe -d <distro> --cd <path>`. | Machine help needs a Windows process. Devs in WSL keep their repos there. |
| Shared folders | An explicit list in the app, each folder picked with the Windows folder dialog. Nothing is scanned. Only places that can run are advertised. | Nothing is shared unless its owner said so, and nobody's folders follow one layout. |
| Versions | The hub and the app share one version number. An app that finds a different version on the hub updates first. | One team and one hub, so no compatibility matrix. |
| Releases | An Azure DevOps pipeline builds the installer and the hub's image, which carries the installer. | That is where the company's registry and cluster are. |
| CI | GitHub Actions on `windows-latest` only. | It is the only automated check of the platform everyone uses. |
| Public repo | Nothing internal is committed: no hostname, no token, no company name. | The repository is public. |

## Phases

| Phase | What it delivers | File |
|---|---|---|
| 0. Skeleton | The Cargo workspace, the UI, the Tauri shell, the desk boundary, Windows CI | [`phase-0-skeleton.md`](plan/phase-0-skeleton.md) |
| 1. Hub | Joining, chats, events, the stream, admin, retention, backups, deploy | [`phase-1-hub.md`](plan/phase-1-hub.md) |
| 2. Chat | The window as a chat: first run, chats, threads, safe rendering, tray, notifications | [`phase-2-chat.md`](plan/phase-2-chat.md) |
| 3. Shipping | The installer, updates from the hub, the image, the release pipeline | [`phase-3-shipping.md`](plan/phase-3-shipping.md) |
| 4. Your own agent | Places, finding agents, process control, ACP, modes, the activity feed, the setup check | [`phase-4-own-agent.md`](plan/phase-4-own-agent.md) |
| 5. Asking others | Approval cards, handing over the steps, follow-ups, the full help-with-my-machine flow | [`phase-5-asking-others.md`](plan/phase-5-asking-others.md) |
| 6. Refinements | The folder dialog, setting up an agent, honest tags, notifications for what waits on you, small fixes | [`phase-6-refinements.md`](plan/phase-6-refinements.md) |
| 7. One approval | No modes, only tags reach agents, following threads, the hub on Postgres | [`phase-7-one-approval.md`](plan/phase-7-one-approval.md) |

## Not doing

- macOS and Linux apps.
- Signed events, agent keys, an audit chain.
- Standing grants, agents asking agents, workflows.
- Branch rooms, patches, Azure DevOps pull requests and builds.
- An MCP server, a command line for chatting, search, reactions, edits and deletes.
- Single sign-on.
- Running anything elevated, or handling a password.
- One-shot agent modes; ACP only.
- Modes for a run, and approving each of its steps.

## Not decided

Raised but neither accepted nor declined. Do not build these without asking:

- A second machine for one person: today a name has one token, so one machine.
- Deployed agents: a headless `player-two-desk` binary on a server, over the same desk crate.
- Hiding secrets in a run's activity beyond the masking in phase 6.
- A one-line description on each place, shown in the `@` list.
- A chat with yourself, for your own runs.
- Search across chats.
- An end-to-end test that drives the window.
- Nightly rustfmt, for the style guide's import grouping.
- Resuming an agent's own session after a restart, where the agent supports it.
