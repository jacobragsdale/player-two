# Phase 1: Hub

The whole hub, tested end to end and ready to deploy. [`protocol.md`](protocol.md) is the reference for every task here.

## Tasks

- [x] **P1-1 The wire types.** `crates/core` holds the names, chat ids, the event envelope, every `kind`'s body, the join code, and the API's requests and responses. All of them use serde. Tests pin the JSON shape of each against `protocol.md`.
- [x] **P1-2 The database.**
  - Open it with the pragmas in [Storage](protocol.md#storage), and refuse to start on a file that fails `quick_check`.
  - Build the schema, with `user_version` migrations.
  - Test that a damaged file is refused.
- [x] **P1-3 Joining.**
  - `POST /join` checks the name and the join token, stores the SHA-256 of a new personal token, and refuses a taken name with the reset hint.
  - Bearer-token middleware sits on every person route.
- [ ] **P1-4 Channels and direct chats.**
  - Create, join, leave and list channels.
  - Direct chat ids.
  - Who may read which chat.
- [ ] **P1-5 Events.**
  - `POST /events` enforces the "who may post it" column of the [events table](protocol.md#events). That includes `run.answer` from the asker only when the run's `steps_by` is `asker`.
  - `GET /chats/{chat}/events` returns pages.
- [ ] **P1-6 The stream.**
  - `GET /stream` sends events and presence, resumes with `Last-Event-ID`, and sends keep-alives.
  - A person is here while a stream is open.
- [ ] **P1-7 Places and people.** `PUT /places` and `GET /people`.
- [ ] **P1-8 Admin.**
  - `/admin/join-code` and `/admin/reset`, checked against `PLAYER_TWO_ADMIN_TOKEN`. A reset closes that person's open stream.
  - `player-two-hub join-code` and `player-two-hub reset NAME` call the running hub.
- [ ] **P1-9 Retention and backups.**
  - The nightly job from [Storage](protocol.md#storage).
  - A test with a clock the test controls covers it.
- [ ] **P1-10 Version and downloads.**
  - `/healthz` and `/version`.
  - `/download` serves the installer in `PLAYER_TWO_RELEASE`, and `/update` builds the manifest from it and its `.sig`.
  - Without those files, both return `404`.
- [ ] **P1-11 The image and manifests.** Use `TODO` placeholders for every internal name.
  - `deploy/Dockerfile`: a static musl build of `player-two-hub`, run as a non-root user, with `/data`, `/backups` and `/app/release`.
  - `.dockerignore`.
  - `deploy/k8s/data.yaml`: static NFS PersistentVolumes with `persistentVolumeReclaimPolicy: Retain` and their claims, one for data and one for backups. It is applied once and never deleted.
  - `deploy/k8s/hub.yaml`:
    - a Deployment with 1 replica and the `Recreate` strategy;
    - secrets for both tokens;
    - a Service;
    - an Ingress with a one-hour read timeout and buffering off for the stream.
- [ ] **P1-12 One scenario test.**
  - Two people join, chat in a channel and directly, and one of them reconnects and resumes their stream.
  - An admin resets a name, and that person joins again.
  - A third person cannot read either chat.

## Done when

The scenario test passes, the image builds, and the manifests apply cleanly to a scratch cluster or pass `kubectl apply --dry-run=client`.

## Notes
