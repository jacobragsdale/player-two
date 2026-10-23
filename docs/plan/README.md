# How to build from this plan

This folder is the implementation plan. It is written for a coding agent working on a Linux machine, with a person checking in between phases. [`../plan.md`](../plan.md) holds the scope and the decisions; the files here hold the work.

## The files

| File | What it is |
|---|---|
| [`protocol.md`](protocol.md) | Reference: names, the hub's API, the events, places and modes, the desk boundary, storage. The phase files point here instead of repeating it. |
| [`phase-0-skeleton.md`](phase-0-skeleton.md) | Workspace, UI, Tauri shell, desk boundary, CI. |
| [`phase-1-hub.md`](phase-1-hub.md) | The hub, end to end, with its deploy manifests. |
| [`phase-2-chat.md`](phase-2-chat.md) | The window as a working chat. |
| [`phase-3-shipping.md`](phase-3-shipping.md) | Installer, updates, image, release pipeline. |
| [`phase-4-own-agent.md`](phase-4-own-agent.md) | Running your own agent. |
| [`phase-5-asking-others.md`](phase-5-asking-others.md) | Running someone else's request, with their approval. |

## Order

Do the phases in order, and the tasks within each in order. A phase is finished when every task is ticked, apart from the **(workstation)** ones, and its "Done when" holds. Then stop and report: what was done, the workstation tasks waiting for a person, and any notes. Do not start the next phase in the same sitting unless told to.

## Working rules

1. **One task, one commit.** Tick the task's box in the same commit. The subject is one plain sentence saying what changed.
2. **Green before every commit.** Run the checks in [`AGENTS.md`](../../AGENTS.md). Zero warnings. Do not weaken any configuration to get there.
3. **Windows CI is the check for Windows code.** This machine cannot run the desk's Windows paths or show the window. Push your work branch (never `main`) and wait for CI to go green before ticking a task that touches them. Do not push for any other reason.
4. **Smallest thing that satisfies the task.** No crate or npm package the task does not need, no abstraction with one use, no setting for a value that never changes. Say in the commit why a new dependency is needed.
5. **Each piece of logic leaves one test behind.** Prefer one scenario test against a real hub running in the test over many unit tests with mocks.
6. **Lifting from v0.7.1** means reading the old code (`git show v0.7.1:<path>`) and writing what this design needs, not porting it line by line. Its behaviour around signing, grants, delegation, patches and repos-as-agents does not apply.
7. **Never weaken what Player Two protects.** If a task seems to need the rule in [`../plan.md`](../plan.md) bent, stop and ask.
8. **Scope.** Build only what the tasks say. Anything else you think is needed goes under "Not decided" in [`../plan.md`](../plan.md), with a line on why.
9. **When the plan is wrong, say so.** Write what you found under "Notes" at the end of the phase file, do the nearest correct thing, and carry on.

## Environment

- Rust stable with clippy and rustfmt, and Node 26 with npm. Nothing needs root.
- The app crate builds here against WebKitGTK, but there is no display to run it on. Windows CI and the workstation tasks are where it runs.
- The tests need no network access.
