# Phase 2: Chat

The window becomes a chat people can use every day, without agents yet.

## Tasks

- [x] **P2-1 First run.**
  - The desk decodes a join code, prefills the name from the Windows user name, and joins.
  - It saves `identity.json`.
  - A taken name shows the hub's hint: ask an admin to run `reset`.
- [x] **P2-2 The desk's hub client.**
  - It uses `reqwest` and reads the stream, reconnecting with backoff and `Last-Event-ID`.
  - The desk holds the chat state (chats, unread counts, threads, presence) and sends changes to the window as `Event`s.
- [x] **P2-3 The first-run page.** One field, for the join code, and the name prefilled. It must read clearly to someone who is not technical.
- [x] **P2-4 The chat.**
  - The chat list, the open chat, threads and the message box.
  - Lift v0.7.1's `Sidebar`, `Thread`, `Message` and `Composer` (`src/window/renderer/`), cut down to what v1 has. Leave out reactions, edits, deletes, search and grants.
- [x] **P2-5 Safe rendering.**
  - A person's message is drawn as text.
  - An agent's Markdown is drawn with `react-markdown`, with raw HTML left as text and no image fetched.
  - Vitest tests show that `<script>`, raw HTML, `javascript:` links and remote images all render inert.
- [x] **P2-6 Channels.** Dialogs to create, join and leave a channel, and to start a direct chat.
- [x] **P2-7 Tray, start at login, notifications.**
  - Closing the window keeps the app running in the tray, and the tray's Quit really quits.
  - The app starts at login.
  - A notification arrives for a direct message, or a tag of you or your places, and opens that chat when clicked.
- [x] **P2-8 Version check.** On connecting, a hub with a different version from the app's stops the desk and makes the window say so. Phase 3 turns this into an update.
- [ ] **P2-9 (workstation)** Two people on work machines chat through a real hub in a channel and directly, and the tray, notifications and start at login all behave.

## Done when

Two people can join with a join code and chat. Messages from other people cannot run anything in the window.

## Notes

- **P2-6: a direct chat needs no dialog.** Everyone who has joined is listed under People, each row their direct chat, so starting one is clicking a name.
- **Seeing the window on this machine.** There is no display here, so the window was checked in headless Chromium: the built page with Tauri's IPC mocked by a canned desk, under the same content security policy, in light and dark. The harness lives outside the repository; nothing in the plan asks for one.
