# Phase 3: Shipping

The hub hands out the app and keeps everyone on its version. This comes before agents so that real people can install and chat early.

## Tasks

- [ ] **P3-1 The installer.**
  - An NSIS installer with `installMode: currentUser`, so no admin rights are needed.
  - WebView2 is assumed present (`webviewInstallMode: skip`).
  - It is named `player-two-setup.exe`.
- [ ] **P3-2 Updates.**
  - Use `tauri-plugin-updater`, with its public key in the app's configuration.
  - Its endpoint is set at run time to the hub's `/update`, from `identity.json`, so no address is built in.
  - When the hub's version differs, the window offers the update, installs it and restarts.
  - The webview cannot reach the updater except through the `desk` command.
- [ ] **P3-3 One version.**
  - The workspace version in `Cargo.toml` is the only version number. The app, the hub and `ui/package.json` take it from there.
  - A test fails if they disagree.
- [ ] **P3-4 The image carries the installer.** `deploy/Dockerfile` copies `deploy/release/` (the installer and its `.sig`) to `/app/release`.
- [ ] **P3-5 The release pipeline.**
  - Fill in `build.yaml`'s placeholder steps with the real commands: build the UI, run `cargo tauri build` with the updater's signing key from the variable group, build the hub, copy the installer into `deploy/release/`, then build and push the image.
  - Leave `TODO` only for the owner's internal names.
  - Fail the pipeline when the tag is not `v` followed by the workspace version.
- [ ] **P3-6 The README.** Short sections on:
  - running a hub, with the manifests, the secrets and `join-code`;
  - releasing (bump the version, tag, run the pipeline, `kubectl set image`);
  - joining (download, install, paste the code).
- [ ] **P3-7 (workstation)**
  - Run the pipeline and deploy the image.
  - On a work machine without admin rights, install from the hub's `/download`, then join.
  - Deploy a newer version and watch the app update itself.
  - Note what SmartScreen and Defender do.

## Done when

A person given only the download link and a join code can install, join, and later update, without help and without admin rights.

## Notes
