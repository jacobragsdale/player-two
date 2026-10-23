# Phase 5: Asking others

A teammate's tag runs on your machine only after you approve it, and they can help you through it. This is the help-with-my-machine flow from [`../plan.md`](../plan.md).

## Tasks

- [x] **P5-1 Tagging others.**
  - The list that opens on `@` shows people and their places, from `GET /people`. Lift the ranking ideas from v0.7.1's `Composer`.
  - A tag of someone with no such place, or of nobody, gets a line under the message saying so.
- [x] **P5-2 The approval card.** The owner sees:
  - who asked, and the text;
  - the mode;
  - where it runs, only when the place has both Windows and WSL;
  - the warning that everything the agent reads or prints is shown in this chat;
  - for a checkout, a warning when it has uncommitted changes.

  The owner can Approve or Decline. The asker can Withdraw until the owner acts.
- [x] **P5-3 Running a request.**
  - Approving posts `run.accepted`, and the run goes as in phase 4.
  - Everyone in the chat sees the state line, the activity and the answer.
  - Only the owner can Stop.
- [x] **P5-4 Handing over the steps.**
  - In ask mode, "Let NAME approve the steps" sets `steps_by: "asker"`.
  - The prompt cards then appear for the asker, and the desk accepts `run.answer` only from them.
  - The owner still sees everything and can Stop.
- [x] **P5-5 Follow-ups.**
  - The asker's messages in the request's thread go to the same agent session.
  - Each follow-up waits for a click, unless the owner ticked "Let NAME keep going", which holds until Stop or an hour of quiet.
  - After the app restarts, a follow-up starts a fresh session, given the thread's text.
- [x] **P5-6 Waiting requests.**
  - Requests that arrived while the owner's app was closed are shown when it opens.
  - None of them runs until clicked, `keep_going` included, since it lapses after an hour anyway.
- [x] **P5-7 The scenario test.** It uses the fake agent:
  1. Jacob asks `@alice` to fix pip's proxy.
  2. Alice approves at ask mode and hands the steps to Jacob.
  3. Jacob allows one step and rejects one.
  4. A follow-up runs under keep going.
  5. Alice stops.
  6. A later follow-up waits for her.
  7. A third person in the chat sees all of it but cannot answer a prompt.
- [ ] **P5-8 (workstation)** The same with two real people on work machines and Cursor, on a real proxy or Python problem.

## Done when

A developer can help a less technical colleague fix something on their machine through Player Two. The colleague stays in charge throughout, and both can see what happened.

## Notes
- **The window's parts of P5-1 to P5-6 landed together**, in one commit after the desk's: the `@` list and the lines for tags that name nobody, the approval card with handing over the steps and keep going, the follow-up card, and Withdraw. A request that arrived while the app was closed shows its card when the app opens, and nothing runs until the owner approves it: keep going lives only in a session the running app kept.
