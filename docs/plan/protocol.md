# Protocol reference

The phase files point here. When code and this page disagree, fix whichever is wrong in the same commit.

## Names

- **Person:** `[a-z][a-z0-9-]{0,31}`. At first run it is prefilled from the Windows user name, lowercased, with anything else turned into `-`.
- **Channel:** `#` followed by `[a-z0-9-]{1,32}`.
- **Direct chat:** the two people's names in sorted order, joined by `+`, as in `alice+bob`.
- **Place:** where an agent can run for someone.
  - `machine` is their machine.
  - A folder name, `[a-z0-9._-]{1,64}`, is one of their shared folders. It defaults to the last part of a git checkout's `origin`, or else the folder's own name, lowercased. The owner can rename it.
  - A desk advertises `machine` when it found an agent on any system, and a folder when it found one on the folder's system and the folder is there.
- **Tag:** `@bob` is Bob's `machine` place, and `@bob/uploader` is his `uploader` place.
- **Where a place runs:** `windows`, or `wsl:<distro>`. This is the owner's business. The approval card shows it to the owner, and the chat sees it only in a run's state line.
- **Join code:** `p2-` followed by the unpadded base64url of `{"hub": "<hub address>", "token": "<join token>"}`.

## The hub's API

Everything is JSON. Requests made as a person carry `Authorization: Bearer <personal token>`. Admin requests carry `Authorization: Bearer <admin token>`.

| Method and path | Auth | What it does |
|---|---|---|
| `GET /healthz` | none | `200` once the database is open. |
| `GET /version` | none | `{ "version": "<the hub's version>" }`. |
| `GET /download` | none | The Windows installer the image carries. |
| `GET /update` | none | Tauri's updater manifest, built per request from the installer and its `.sig`. The `url` field is `PLAYER_TWO_URL` + `/download`. |
| `POST /join` | none | `{ join_token, name }` returns `{ token }`. A wrong token gets `403`. A taken name gets `409`, with a message saying an admin can run `player-two-hub reset NAME`. |
| `GET /people` | person | `[{ name, here, last_seen, places: [{ name, kind }] }]`. `kind` is `machine` or `repo`. |
| `PUT /places` | person | `[{ name, kind }]` replaces the caller's advertised places. |
| `GET /channels` | person | `[{ name, members }]`. |
| `POST /channels` | person | `{ name }` creates a channel and joins it. |
| `POST /channels/{name}/join`, `.../leave` | person | Join or leave a channel. |
| `GET /chats/{chat}/events?before={id}&limit={n}` | person | One page of a chat, oldest first. `n` is at most 200. |
| `POST /events` | person | `{ chat, thread?, kind, body }` returns `{ id, ts }`. The hub checks who may post it (see below). |
| `GET /stream` | person | Server-sent events (see below). |
| `POST /admin/join-code` | admin | `{ code }`. |
| `POST /admin/reset` | admin | `{ name }` drops that person's token so they can join again under the same name. It keeps their chats and closes their open stream. |
| `POST /admin/remove` | admin | `{ name }` drops that person's token, takes them out of every channel and off `GET /people`, and closes their open stream. What they wrote stays. Joining again under the name brings them back. |

A channel's events go to its members, and a direct chat's to its two people. Nobody else can read them.

### The stream

`GET /stream` stays open. Each event the person may see is sent as `id: <event id>` and `data: <event JSON>`. Presence changes are sent as `event: presence` and `data: { name, here }`. A comment line keeps the stream alive every 20 seconds. On reconnecting, the desk sends `Last-Event-ID` and gets everything after it.

A person counts as **here** while one of their streams is open. `last_seen` is when their last stream closed. When someone changes their places, everyone is sent their presence again, as a sign to fetch `/people` anew.

## Events

Every event has the same envelope: `{ id, chat, thread, author, kind, body, ts }`.
- The hub assigns `id`, which only ever increases, and `ts`, from its own clock in Unix milliseconds.
- `thread` is the `id` of the thread's first message. It is absent for an event outside a thread.

| `kind` | `body` | Who may post it |
|---|---|---|
| `message` | `{ text, tags: [{ person, place }] }`. `text` is at most 64,000 characters. | Anyone in the chat. |
| `run.accepted` | `{ request, place, runs_in, keep_going }`. `request` is the message being answered. | The owner of the tagged place, or for a follow-up, the owner of the run it follows. |
| `run.declined` | `{ request }` | The owner of the tagged place, or for a follow-up, the owner of the run it follows. |
| `run.withdrawn` | `{ request }` | Whoever wrote the request. |
| `run.activity` | `{ run, items: [{ id, verb, object, outcome?, output? }] }`. `run` is the `id` of its `run.accepted`. | The owner. |
| `run.result` | `{ run, text }`: the agent's answer, as Markdown. | The owner. |
| `run.ended` | `{ run, how, reason? }`. `how` is `done`, `stopped` or `failed`. | The owner. |
| `run.stop` | `{ run }`: the asker wants the run stopped. The owner's desk stops it as if the owner had pressed Stop. | The asker: whoever wrote the request the run answers. |

Rules every desk keeps, as well as the hub:

- **Only the sender's desk resolves tags,** and only in a `message` that a person typed. Nobody looks for tags in an agent's output.
- **A follow-up is a `message` in a request's thread,** written by the original asker. It needs a new `run.accepted` from the owner. The owner's desk posts that by itself only while the thread's `keep_going` holds: until a `run.ended` with `how: "stopped"`, or until an hour passes with no event in the thread.
- **Your own tag** sent from your window produces the same events, and runs at once.
- **Each place has at most one active run.** A second request stays waiting, and its card says the place is busy and with whose request.
- **Credentials are masked.** Before posting `run.activity`, `run.result` or the reason of a `run.ended`, the owner's desk replaces with `****` the password in a URL's `user:password@`, and the value after `password=`, `token=` or `Authorization:`, in any case.

## Runs

A run its owner let go may do anything their user can. Each agent starts in its own mode that asks nobody: Cursor as `agent --force acp`, Copilot as `copilot --acp --stdio --allow-all`, and an agent whose session offers a `bypassPermissions` mode, as Claude Code's adapter does, is put in it with `session/set_mode`. The setup check starts an agent the same way. The desk answers each `session/request_permission` that still comes yes, choosing the option that allows it once where the agent offers one, since an agent may keep an "always" in its own settings. The desk offers the agent no file system or terminal capability of its own: the agent uses its own tools. Player Two does not sandbox the agent: it runs as the owner's Windows or WSL user.

Every run starts with a preamble ahead of the request's text. It says:
- whose machine this is and who asked;
- that it may do what the request needs;
- that the agent must never try to elevate (no UAC, `runas` or `sudo`). If a step needs admin, it must stop and say exactly what the owner should run;
- that the request is from the asker, not from the owner.

## The desk boundary

The window reaches the desk through one Tauri command and one event:
- the command `desk` takes a `Request` and returns a `Response`;
- the event `desk` carries an `Event`.

The three enums live in `crates/desk/src/ui.rs`. A test writes them to `ui/src/desk.ts` with `specta-typescript`, and fails when the file it finds is out of date. Nothing else crosses between the webview and Rust.

The desk keeps its files in the app's data folder, which Tauri names after the app's identifier (`%APPDATA%\io.github.jacobragsdale.player-two\`):
- `identity.json`: `{ hub, name, token }`.
- `settings.json`: the shared folders with their former names, the agent chosen for each system, and where `machine` runs by default.
- `found.json`: the agents and missing folders the desk found when it last looked, which it advertises from.

## Storage

The hub keeps its data in a Postgres database of its own, at `PLAYER_TWO_DATABASE_URL`, over one connection, which every request takes in turn. Events are therefore written one at a time, and their ids come in the order they were written. The hub stops, saying why, when it cannot connect or loses the connection; Kubernetes starts it again. Nothing else uses the database. The admin commands are HTTP calls to the running hub.

| Table | Columns |
|---|---|
| `people` | `name` (key), `token_sha256`, `joined`, `last_seen`, `removed` |
| `channels` | `name` (key), `created_by`, `created` |
| `members` | `channel`, `name` |
| `events` | `id` (key, an identity), `chat`, `thread`, `author`, `kind`, `body` (JSON), `ts` |
| `places` | `owner`, `name`, `kind`; the key is `owner` and `name` together |

The one row of `schema_version` holds the schema's version, and the hub brings an older database forward on start.

Every night at 03:00 UTC, the hub deletes each thread whose newest event is older than `PLAYER_TWO_RETENTION_DAYS`, whole, so that no reply or run is left without its message. A message outside any thread is a thread of its own. Backups are the database instance's.

## The hub's environment

| Variable | Default | What it is |
|---|---|---|
| `PLAYER_TWO_DATABASE_URL` | none, required | The hub's database, as `postgres://user:password@host/database`. |
| `PLAYER_TWO_JOIN_TOKEN` | none, required | The shared join token. |
| `PLAYER_TWO_ADMIN_TOKEN` | none, required | For `/admin/*`. |
| `PLAYER_TWO_URL` | none, required | The hub's own address, for join codes and the update manifest. |
| `PLAYER_TWO_RETENTION_DAYS` | `30` | How long events are kept. |
| `PLAYER_TWO_RELEASE` | `/app/release` | The folder holding `player-two-setup.exe` and `player-two-setup.exe.sig`. |
| `PLAYER_TWO_PORT` | `8787` | Where the hub listens, and where the admin commands look for it. |

`player-two-hub serve` runs the hub. `player-two-hub join-code`, `player-two-hub reset NAME` and `player-two-hub remove NAME` call the running hub at `http://127.0.0.1:<PLAYER_TWO_PORT>` with `PLAYER_TWO_ADMIN_TOKEN`. In the cluster you run them as `kubectl exec deploy/player-two -- player-two-hub join-code`.
