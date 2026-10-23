# Player Two

A chat for a small team and the coding agents on their own Windows machines. You can ask a teammate's agent to fix your proxy, or drive your own agent in a channel while others watch. Nothing runs on anyone's machine until its owner says so.

It has two parts. The **hub** is one small server the team shares, holding the chats in one SQLite file. The **app** is a Windows program, built with Tauri, that each person installs; it keeps their identity, talks to the hub, and runs their agent when they say so.

The plan, with what v1 does and does not do, is in [`docs/plan.md`](docs/plan.md).

## Joining

You need two things from whoever runs the hub: its download link and a join code.

1. Open the download link, `https://<hub>/download`, and run `player-two-setup.exe`. It installs for you alone and needs no admin rights.
2. Paste the join code into the window that opens, check your name, and press **Join**.

Player Two starts with Windows and lives in the tray. Closing the window keeps it running; **Quit** in the tray icon's menu stops it.

Right after joining, Player Two helps you set up the agent it runs on your computer. **Set up Cursor** installs Cursor's agent for you, without admin rights; Copilot and Claude Code have steps to copy. You can skip it, and **Settings** has the same, with a **Check** that the agent is ready.

If you work in WSL, install an agent inside your distro too, for Cursor's with `curl https://cursor.com/install -fsS | bash`, then press **Look again** in **Settings**.

## Using it

- **Ask your own agent** by tagging yourself: `@you what is eating my disk space?` It runs at once, on your computer, at the mode under the message box: **Read-only**, **Ask** (each change or command waits for your yes), or **Full**.
- **Ask a teammate's agent** by tagging them: `@alice can your agent fix pip's proxy?` Nothing runs until Alice approves. Her card lets her choose the mode, hand you the steps to answer, and let you keep going: your replies in that thread then run without asking her again, until she presses **Stop** or the thread is quiet for an hour. You can press **Stop** on a run you asked for, too.
- **Share a folder** in **Settings** so teammates can ask about it as `@you/name`. **Share a folder…** opens Windows' folder dialog; a folder in WSL is under **Linux** in its side panel. A git checkout is named after its remote, and any other folder after itself.
- Everyone in the chat sees what the agent did and what it answered. Nothing runs as administrator: when a step needs that, the agent says what to run.

## Running a hub

The hub is one container, run on Kubernetes with its files on NFS. The manifests are in [`deploy/k8s/`](deploy/k8s/); fill in each `TODO` first.

1. On the NFS server, make the two folders that `deploy/k8s/data.yaml` names, writable by user 10001.
2. Make the secret with the two tokens:

   ```sh
   kubectl create secret generic player-two \
     --from-literal=join-token=$(openssl rand -hex 24) \
     --from-literal=admin-token=$(openssl rand -hex 24)
   ```

3. Apply `data.yaml` once, and never delete it. Then apply `hub.yaml`.
4. Print a join code to send people:

   ```sh
   kubectl exec deploy/player-two -- /app/player-two-hub join-code
   ```

If someone's machine is replaced, they cannot join again under their name until an admin runs `player-two-hub reset NAME` the same way. Their chats are kept.

The database is copied every night at 03:00 UTC to the backups folder, and the newest seven copies are kept. To restore one, scale the deployment to 0, copy it over `player-two.db` in the data folder, delete any `player-two.db-wal` there, and scale back to 1. Messages older than 30 days are deleted; `PLAYER_TWO_RETENTION_DAYS` changes that.

## Releasing

The hub and the app share one version number, set in the workspace's `Cargo.toml` and repeated in `ui/package.json`.

1. Change the version in both places, commit, and tag the commit `vX.Y.Z`.
2. The Azure DevOps pipeline in [`build.yaml`](build.yaml) builds the installer, signs it for the updater, and builds and pushes the hub's image, which carries the installer.
3. Point the deployment at the new image:

   ```sh
   kubectl set image deployment/player-two hub=<registry>/<repository>:X.Y.Z
   ```

Each app finds the hub on another version the next time it connects, and offers to update.

## Developing

[`AGENTS.md`](AGENTS.md) has the layout and the checks every commit passes. In short, from the repository's root:

```sh
npm --prefix ui ci
npm --prefix ui run check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The previous version, in TypeScript and Electron, is tagged [`v0.7.1`](../../tree/v0.7.1).
