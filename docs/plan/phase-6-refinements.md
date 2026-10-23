# Phase 6: Refinements

Shared folders are picked with the folder dialog, setting up an agent is part of joining, and the small things found in a pass over the app are fixed. Agreed on 2026-10-23. [`protocol.md`](protocol.md) is the reference.

## Tasks

- [x] **P6-1 Picking a folder.**
  - "Add folder" opens the Windows folder dialog. The window sends the desk a `pick_folder` request, and the desk opens the dialog through a picker the app shell gives it at start. The picker is a crate such as `rfd`, not a Tauri plugin, and `main.json` stays as it is.
  - A path under `\\wsl.localhost\<distro>\` or `\\wsl$\<distro>\` is that distro's folder, and the rest is its Linux path.
  - The text box, the "which system" dropdown, the scan of `dev` folders and `Request::Suggestions` go.
  - Any folder can be shared. A git checkout is named from its `origin`, and any other folder by its own name. The window says "shared folder", not "checkout".
  - The README says how.
- [x] **P6-2 Renaming a place.**
  - Settings lets the owner rename a shared folder. The desk checks the name and advertises the places again.
  - The desk keeps each folder's former names in `settings.json`, so that a waiting request or a follow-up addressed to an old name still finds it. Tags read only current names, and no other folder is given a former name.
- [x] **P6-3 Advertising only what can run.**
  - `machine` is advertised when an agent was found on any system, and a folder when one was found on its system and the folder is there.
  - The desk keeps what it last found in `found.json` and advertises from it.
  - At start it looks on Windows only. It looks in WSL again only when Settings opens, an approval card appears or a run starts, so the app never boots WSL at login.
  - Docker Desktop's distros, `docker-desktop` and `docker-desktop-data`, are never searched or offered.
  - Settings keeps a folder that is gone, or whose distro has no agent, and marks it as not available.
- [x] **P6-4 Tags that name nothing say why.** The line under the message says which it is: nobody has that name, the person does not share that place, or the person has no agent set up yet. A person with no places advertised has none.
- [x] **P6-5 Choosing the agent.** When a system has more than one agent, Settings lets the owner choose which runs there, with Cursor the default when it was found. The choice is kept per system in `settings.json`. `custom_agent` stays a setting in the file only.
- [x] **P6-6 Setting up an agent.**
  - After Join, one step that can be skipped asks which agent the person uses. Settings has the same section. An agent already installed is chosen for them, and the step goes straight to Check.
  - Cursor: a Set up button makes the desk run the official installer, `irm 'https://cursor.com/install?win32=true' | iex`, and shows what it prints. The command is fixed in the desk, never sent by the window. It is also shown to copy, for when the button fails.
  - Copilot: steps to copy, `winget install GitHub.Copilot`, and a note that it needs PowerShell 7.
  - Claude Code: steps to copy, `npm install -g @agentclientprotocol/claude-agent-acp`, and a note that it needs Node.
  - Half installed: the Cursor editor (`cursor` on the `PATH`) without its CLI, or `claude` without the adapter, gets a line saying what is missing.
  - Someone with a WSL distro, not counting Docker Desktop's, sees one collapsed line: install an agent inside the distro, with Cursor's `curl https://cursor.com/install -fsS | bash` to copy, then press Look again. The README says the same.
- [x] **P6-7 Signing in over ACP.** A Sign in button starts the agent and, when its `initialize` offers an auth method, calls `authenticate` with it (`cursor_login` for Cursor), which opens the browser. The setup check uses the same. An agent that offers none shows its sign-in command, as now.
- [x] **P6-8 People.**
  - A direct chat's header shows whether the person is here, "last seen 2 h ago" when not, and the places they share.
  - The People list puts the people you have messaged first, most recent first, then everyone else by name.
- [x] **P6-9 Notifications for what waits on you.**
  - These notify too: a follow-up waiting for your click, a step waiting for your Allow (whether you own the run or were handed its steps), and the end of a run you asked for.
  - `Event::Open` carries the thread, and the window opens it.
- [x] **P6-10 Run now.** When your own tag found its place busy, the waiting line gets a Run now button once the place is free. The button names the mode it will run at, which is your default when you click. There is still no queue.
- [x] **P6-11 The mode switch where it matters.** It shows under the message box only while the text tags one of your places, or in the thread of a run of yours.
- [x] **P6-12 The asker can stop.** Whoever asked gets a Stop button that posts `run.stop`. The owner's desk stops the run as if the owner had pressed Stop, and the `run.ended` says who stopped it. The hub checks who may post it.
- [x] **P6-13 Masking credentials.** Before posting `run.activity`, `run.prompt`, `run.result` or the reason of a `run.ended`, the owner's desk replaces with `****` the password in a URL's `user:password@`, and the value after `password=`, `token=` or `Authorization:`, in any case. Tests cover each, and show that ordinary text is left alone.
- [x] **P6-14 Shorter activity.** While a run works, its latest 5 steps show, and once it ends, none. "Show all N steps" shows the rest. Prompts and the answer always show.
- [x] **P6-15 Small things.**
  - The window opens the chat that was open when it was last closed.
  - The first time the window is closed, a notification says Player Two is still running in the tray.
  - Leaving a channel asks first.
  - The tray menu has Settings.
- [ ] **P6-16 (workstation)** On a work machine with only the Cursor editor installed:
  - pick a Windows folder and a `\\wsl.localhost` folder with the dialog;
  - set up Cursor from the step after joining, without admin rights, and sign in with the button;
  - try Sign in with Copilot and with Claude Code, and write down under "Notes" which of them offer it over ACP;
  - tag a teammate who has no agent and one who has, and check the lines and notifications above.

## Done when

On a work machine, someone with only the Cursor editor joins, sets up Cursor from the app, and shares a folder picked with the dialog. A teammate can tag it, and both are notified of everything that waits on them.

## Notes
- **P6-11: your own follow-up runs at once.** The plan took it that a follow-up in the thread of your own run already ran your agent, so the mode switch belonged there. It did not: it showed you an approval card for your own request. It now runs at once at your mode, as your own tag does, and waits as before when the place is busy.
