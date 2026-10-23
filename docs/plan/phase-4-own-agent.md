# Phase 4: Your own agent

You tag your own agent and watch it work, on Windows or in WSL. [Modes](protocol.md#modes) and [Events](protocol.md#events) are the reference.

## Tasks

- [ ] **P4-1 Places.**
  - A settings page lists `machine` and the shared folders.
  - "Add folder" takes a Windows path, or a WSL path when WSL with an agent was found.
  - Suggestions come from scanning `~/dev` in each WSL distro and `%USERPROFILE%\dev`.
  - A folder is named after the last part of its `origin`. Lift the URL handling from v0.7.1's `src/core/remotes.ts`.
  - The desk advertises the places with `PUT /places`.
- [ ] **P4-2 Finding agents.**
  - On Windows, look on the `PATH` for `agent`, `copilot` and the Claude Code ACP adapter, including npm's `.cmd` shims.
  - In each distro from `wsl.exe -l -q`, check with `wsl.exe -d <distro> -- bash -lc 'command -v …'`.
  - Each place records its agent and where it runs.
  - With no WSL, or no agent in it, nothing in the window mentions WSL.
- [ ] **P4-3 Starting and stopping.**
  - Start an agent with piped stdio, directly on Windows (resolving `.cmd` shims) or with `wsl.exe -d <distro> --cd <path> -- bash -lc 'exec …'`.
  - Stop kills the whole tree: with a Job Object on Windows (in `crates/win` only if no crate covers it), and with the process group elsewhere.
  - Tests use a child that starts a grandchild, and must pass on both systems.
- [ ] **P4-4 ACP.** Use the `agent-client-protocol` crate:
  - initialize, then open a session in the place's folder;
  - send the preamble from [Modes](protocol.md#modes) and the prompt;
  - turn session updates into activity items, with the verb-object-outcome wording lifted from v0.7.1's `src/desk/agents/activity.ts`;
  - support cancel.
  - An agent that says it must sign in fails the run with that agent's sign-in command.
- [ ] **P4-5 The fake agent.** A test binary that speaks ACP and follows a script: it reads, edits, runs a command, asks permission, fails, or needs signing in. It is the agent in every desk test from here on.
- [ ] **P4-6 Modes.** Answer `session/request_permission` as the [Modes table](protocol.md#modes) says. In ask mode, wait for the window's answer.
- [ ] **P4-7 Your own runs.**
  - Your own tag runs at once at your default mode, posting `run.accepted`, `run.activity`, `run.result` and `run.ended`.
  - The message box shows the mode, with a one-click switch to full that becomes the new default.
  - Stop ends the run.
- [ ] **P4-8 One at a time.** A place that is running something says so, and a second tag of it waits.
- [ ] **P4-9 The setup check.** For each place, the window checks for its agent and whether it is signed in. If not, it gives the one command to install it or sign in (for Cursor on Windows, `irm 'https://cursor.com/install?win32=true' | iex` and then `agent login`). Lift the idea, not the code, from v0.7.1's `src/desk/doctor.ts`.
- [ ] **P4-10 The run in the window.**
  - Under the message: the state line, the live activity feed with outputs, and the answer as Markdown.
  - In ask mode, prompt cards with Allow and Reject.
- [ ] **P4-11 (workstation)** Run your own Cursor on Windows and in WSL, then Copilot, then Claude Code through its adapter, at each mode. Write down what each agent asks permission for, and what it does without asking, under "Notes".

## Done when

On a work machine, you can tag `@you` and `@you/repo`, watch the agent work at each mode, answer its prompts, and stop it.

## Notes
