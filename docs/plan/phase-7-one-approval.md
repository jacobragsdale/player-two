# Phase 7: One approval

A teammate's request waits for one click, and after it the agent asks nobody. Only a tag reaches an agent. The hub keeps its data in Postgres. Agreed on 2026-10-24. [`protocol.md`](protocol.md) is the reference.

## Tasks

- [x] **P7-1 The hub on Postgres.**
  - The hub keeps its data in a Postgres database of its own, at `PLAYER_TWO_DATABASE_URL`, over one connection. It makes and updates its tables at start, with the schema's version in a one-row table.
  - It exits, saying why, when it cannot connect or loses the connection. Kubernetes starts it again.
  - SQLite goes: `PLAYER_TWO_DB`, `PLAYER_TWO_BACKUPS`, `quick_check`, the nightly copies, `deploy/k8s/data.yaml` and its NFS volumes. Backups are the database instance's.
  - Tests make a database of their own in the Postgres at `PLAYER_TWO_TEST_DATABASE_URL`, and drop it after. Without it they fail and say how to start one. Windows CI starts the PostgreSQL the runner has.
  - The README and `AGENTS.md` say how.
- [x] **P7-2 Retention by thread.** The nightly job deletes a thread whole once its newest event is older than `PLAYER_TWO_RETENTION_DAYS`, so a reply is never left without its message.
- [ ] **P7-3 Removing someone.** `player-two-hub remove NAME` drops their token, takes them out of every channel and off `GET /people`, and closes their stream. Their messages stay until retention takes them.
- [ ] **P7-4 No modes.**
  - A teammate's request runs once its owner approves it, and your own tag runs at once. Either may do anything the owner's user can.
  - The mode buttons, the mode switch under the message box, handing over the steps, prompt cards and their notifications go, with `run.prompt`, `run.answer`, and `mode` and `steps_by` in `run.accepted`.
  - The card warns: "It can change files and run commands as you, without asking. Everything it reads or prints is shown in this chat."
- [ ] **P7-5 Asking nobody.**
  - Each agent starts in its own mode that does not ask: Claude Code's adapter is set to `bypassPermissions` with `session/set_mode`, Copilot gets `--allow-all`, and Cursor gets `--force`.
  - The desk says yes to anything that still asks.
  - The setup check starts the agent exactly as a run does.
- [ ] **P7-6 Only Cursor's `agent` is Cursor.** An `agent` on the `PATH` counts as Cursor only when it is in Cursor's own install folder. Any other is ignored.
- [ ] **P7-7 Only a tag reaches an agent.**
  - A follow-up is a message in a request's thread that tags the place again. A message without tags is a comment, and runs nothing, in your own run's thread too.
  - Keep going lets the asker's follow-ups run without a click, until Stop or an hour after the agent last finished in the thread.
  - Anyone's tag of the place in the thread goes to the same session, and they are that run's asker.
  - A fresh session is given only the requests that ran there and the agent's answers, never the comments.
  - A follow-up approved to run on another system starts a fresh session there, not the one kept on the first.
- [ ] **P7-8 Each tag on its own.** Every tag in a message is answered on its own: `run.declined` names the place. Withdraw takes back the whole message.
- [ ] **P7-9 Tags of people not in the channel.** A tag of someone who is not in the channel is not resolved, and the line under the message says "alice is not in #ops, so she cannot see this".
- [ ] **P7-10 Following a thread.** A comment in a thread notifies whoever wrote its first message, anyone who has written in it, and the owner of any run in it, but not its author.
- [ ] **P7-11 The preamble.** It says the agent may do what the request needs, and that everything it reads or prints is shown to everyone in the chat, so it must never print passwords, tokens or keys.
- [ ] **P7-12 Asking before stopping a run.** While a run is going at one of your places, Update and the tray's Quit ask first, naming the run they would stop.
- [ ] **P7-13 A long answer is cut.** An agent's answer longer than an event may hold is cut to fit, and says so, rather than lost.
- [ ] **P7-14 (workstation)**
  - With Cursor: whether `agent acp --force` stops it asking, and whether its permission options spell their `kind` `allow-once`.
  - Where Cursor's installer puts `agent` on Windows, and what `agent --version` prints.
  - With Claude Code in a WSL distro whose user is root: whether `bypassPermissions` is refused, and that the run still goes, with the desk saying yes.
  - Write each down under "Notes".

## Done when

A teammate's request runs on one click without another question, follow-ups and comments do what their tags say, and the hub keeps everything in Postgres.

## Notes
