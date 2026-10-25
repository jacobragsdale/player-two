//! One person's side of Player Two: their hub client, places, agents, approvals and runs.
//!
//! The window asks the desk for things with [`Desk::handle`], and hears what to ask for again
//! from the receiver [`Desk::new`] gives back. [`Desk::run`] keeps the desk connected to the hub
//! for as long as the app runs.

pub mod acp;
pub mod activity;
pub mod agents;
pub mod chats;
pub mod files;
pub mod hub;
mod looking;
pub mod mask;
pub mod places;
pub mod process;
pub mod runs;
pub mod system;
pub mod tags;
pub mod ui;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::Context;
use chats::Chats;
use files::{Files, Identity};
use hub::{Hub, Incoming, NotJoined};
use looking::Where;
use places::{Folder, Looked, Settings};
use player_two_core::api::{NewEvent, PlaceInfo};
use player_two_core::event::RunsIn;
use player_two_core::event::{Body, Event, EventId, Message};
use player_two_core::join_code::JoinCode;
use player_two_core::names::{ChannelName, ChatId, PersonName, PlaceName};
use tokio::sync::{Notify, mpsc};
use ui::{AgentInfo, Connection, Overview, PlacesView, Request, Response, SharedFolder};

/// How many events one page from the hub holds.
const PAGE: u32 = 100;
/// The longest wait between attempts to reconnect.
const MOST_DELAY: Duration = Duration::from_secs(30);
/// How often the people and channel lists are fetched again while connected.
const REFRESH: Duration = Duration::from_secs(60);
/// How long a desk that found the hub on another version waits before looking again.
const RECHECK: Duration = Duration::from_secs(300);

/// Opens the system's folder dialog, and gives back the folder picked, or `None` when the person
/// cancelled it. The app shell gives the desk one with [`Desk::set_picker`].
pub type Picker =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Option<String>> + Send>> + Send + Sync>;

/// The hub runs another version than this app, which must change before anything else.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WrongVersion(String);

impl std::fmt::Display for WrongVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the hub runs {} and this app {}",
            self.0,
            env!("CARGO_PKG_VERSION")
        )
    }
}

impl std::error::Error for WrongVersion {}

/// The desk. Cloning it gives another handle on the same desk.
#[derive(Debug, Clone)]
pub struct Desk {
    inner: Arc<Inner>,
}

struct Inner {
    files: Files,
    http: reqwest::Client,
    events: mpsc::UnboundedSender<ui::Event>,
    state: Mutex<State>,
    /// Rung when the run loop should look again: the person joined, or asked it to.
    wake: Notify,
    /// The runs going on at this machine's places.
    runs: Mutex<runs::Runs>,
    /// Runs stopped, so that their ending says so, with who stopped each when it was not the
    /// person.
    stopped: Mutex<std::collections::HashMap<EventId, Option<PersonName>>>,
    /// The folder dialog, once the app shell has given one.
    picker: Mutex<Option<Picker>>,
    /// Held while the desk looks for agents and folders, so that looks take turns.
    looking: tokio::sync::Mutex<()>,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("files", &self.files)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct State {
    connection: Connection,
    /// What was found when the desk last looked, once read or looked for.
    looked: Option<Looked>,
    /// The places the hub was last told of on this connection.
    advertised: Option<Vec<PlaceInfo>>,
    hub: Option<Hub>,
    chats: Option<Chats>,
}

impl Desk {
    /// A desk keeping its files in `folder`, and the receiver of what it tells the window.
    #[must_use]
    pub fn new(folder: PathBuf) -> (Self, mpsc::UnboundedReceiver<ui::Event>) {
        let (events, receiver) = mpsc::unbounded_channel();
        let inner = Inner {
            files: Files::new(folder),
            http: reqwest::Client::new(),
            events,
            state: Mutex::new(State {
                connection: Connection::Connecting,
                looked: None,
                advertised: None,
                hub: None,
                chats: None,
            }),
            wake: Notify::new(),
            runs: Mutex::new(runs::Runs::default()),
            stopped: Mutex::new(std::collections::HashMap::new()),
            picker: Mutex::new(None),
            looking: tokio::sync::Mutex::new(()),
        };
        (
            Self {
                inner: Arc::new(inner),
            },
            receiver,
        )
    }

    /// Gives the desk the system's folder dialog, which sharing a folder opens.
    pub fn set_picker(&self, picker: Picker) {
        *self
            .inner
            .picker
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(picker);
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // Every change to the state is whole before the lock is let go.
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn tell(&self, event: ui::Event) {
        // A window that has gone away does not need telling.
        let _ = self.inner.events.send(event);
    }

    fn set_connection(&self, connection: Connection) {
        self.state().connection = connection;
        self.tell(ui::Event::Overview);
    }

    /// Answers one request from the window. A failure is said in words for the person.
    pub async fn handle(&self, request: Request) -> Result<Response, String> {
        self.answer(request)
            .await
            .map_err(|error| format!("{error:#}"))
    }

    async fn answer(&self, request: Request) -> anyhow::Result<Response> {
        match request {
            Request::Hello => Ok(Response::Hello {
                version: env!("CARGO_PKG_VERSION").to_owned(),
            }),
            Request::Setup => Ok(Response::Setup {
                joined: self.inner.files.identity()?.map(|identity| identity.name),
                suggested_name: suggested_name().map(String::from),
            }),
            Request::Join { code, name } => {
                self.join(&code, &name).await?;
                Ok(Response::Done)
            }
            Request::Overview => Ok(Response::Overview(self.overview())),
            Request::Chat { chat, thread } => {
                let state = self.state();
                let chats = state
                    .chats
                    .as_ref()
                    .context("the chats have not loaded yet")?;
                Ok(Response::Chat(chats.view(&chat, thread)))
            }
            Request::Older { chat } => {
                self.older(&chat).await?;
                Ok(Response::Done)
            }
            Request::Send { chat, thread, text } => {
                self.send(chat, thread, text).await?;
                Ok(Response::Done)
            }
            Request::RunNow {
                chat,
                request,
                place,
            } => {
                self.run_now(&chat, request, place).await?;
                Ok(Response::Done)
            }
            Request::Stop { chat, thread, run } => {
                let owner = self
                    .state()
                    .chats
                    .as_ref()
                    .and_then(|chats| chats.owner_of(&chat, run));
                if self.runs_going(run) {
                    self.stop_run(run, None)?;
                } else if owner.is_some() && owner == self.me() {
                    // A run of this machine's that is not going on has nothing to stop.
                    anyhow::bail!("that run is not going on here");
                } else {
                    self.post(
                        &chat,
                        Some(thread),
                        Body::RunStop(player_two_core::event::RunStop { run }),
                    )
                    .await?;
                }
                Ok(Response::Done)
            }
            Request::Read { chat } => {
                self.read(&chat)?;
                Ok(Response::Done)
            }
            Request::CreateChannel { name } => {
                let name = ChannelName::new(name.trim().trim_start_matches('#'))?;
                self.hub()?.create_channel(&name).await?;
                self.channels_changed(Some(ChatId::Channel(name))).await?;
                Ok(Response::Done)
            }
            Request::JoinChannel { name } => {
                let name = ChannelName::new(name)?;
                self.hub()?.membership(&name, true).await?;
                self.channels_changed(Some(ChatId::Channel(name))).await?;
                Ok(Response::Done)
            }
            Request::Update => anyhow::bail!("updates are installed by the app, not the desk"),
            Request::Places => {
                let looked = self.look(Where::Everywhere).await?;
                let settings = self.settings()?;
                let found = &looked.agents;
                let mut distros: Vec<String> = found
                    .iter()
                    .filter_map(|agent| match &agent.runs_in {
                        RunsIn::Wsl(distro) => Some(distro.clone()),
                        RunsIn::Windows => None,
                    })
                    .collect();
                distros.dedup();
                let folders = settings
                    .folders
                    .iter()
                    .map(|folder| SharedFolder {
                        name: folder.name.clone(),
                        path: folder.path.clone(),
                        runs_in: folder.runs_in.clone(),
                        unavailable: looked.unavailable(folder),
                    })
                    .collect();
                let runs = |agent: &agents::Found| {
                    let chosen = settings.agents.get(&agent.runs_in).copied();
                    agents::for_system(found, &agent.runs_in, chosen)
                        .is_some_and(|running| running.kind == agent.kind)
                };
                let agents = found
                    .iter()
                    .map(|agent| AgentInfo {
                        kind: agent.kind,
                        runs_in: agent.runs_in.clone(),
                        chosen: runs(agent),
                    })
                    .collect();
                Ok(Response::Places(PlacesView {
                    machine_runs_in: settings.machine_runs_in,
                    folders,
                    distros,
                    agents,
                    half_installed: agents::half_installed(found).await,
                    wsl: system::distros()
                        .await
                        .is_some_and(|distros| !distros.is_empty()),
                }))
            }
            Request::PickFolder => {
                let picker = self
                    .inner
                    .picker
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone()
                    .context("folders are picked with Windows' folder dialog, which is not here")?;
                if let Some(picked) = picker().await {
                    let (path, runs_in) = places::located(&picked);
                    self.add_folder(path, runs_in).await?;
                }
                Ok(Response::Done)
            }
            Request::RenameFolder { name, to } => {
                let mut settings = self.settings()?;
                settings.rename(&name, &to)?;
                self.save_settings(&settings).await?;
                Ok(Response::Done)
            }
            Request::RemoveFolder { name } => {
                let mut settings = self.settings()?;
                settings.folders.retain(|folder| folder.name != name);
                self.save_settings(&settings).await?;
                Ok(Response::Done)
            }
            Request::InstallCursor => {
                let (ok, output) = agents::install_cursor().await?;
                self.look(Where::Here).await?;
                Ok(Response::Installed { ok, output })
            }
            Request::ChooseAgent { runs_in, kind } => {
                let mut settings = self.settings()?;
                settings.agents.insert(runs_in, kind);
                self.save_settings(&settings).await?;
                Ok(Response::Done)
            }
            Request::CheckAgent { runs_in } => {
                let found = self.look(Where::At(runs_in.clone())).await?.agents;
                let chosen = self.settings()?.agents.get(&runs_in).copied();
                let Some(agent) = agents::for_system(&found, &runs_in, chosen) else {
                    return Ok(Response::Checked {
                        ready: false,
                        message: "no agent is installed there".to_owned(),
                        sign_in: false,
                    });
                };
                Ok(check(agent).await)
            }
            Request::SignIn { runs_in } => {
                let found = self.look(Where::At(runs_in.clone())).await?.agents;
                let chosen = self.settings()?.agents.get(&runs_in).copied();
                let agent = agents::for_system(&found, &runs_in, chosen)
                    .context("no agent is installed there")?;
                match acp::sign_in_over_acp(agent, &agent.home).await? {
                    acp::SignedIn::Done => Ok(check(agent).await),
                    acp::SignedIn::NotOffered => Ok(Response::Checked {
                        ready: false,
                        message: format!(
                            "{} cannot be signed in from here: {}",
                            agent.kind.label(),
                            acp::sign_in(agent.kind)
                        ),
                        sign_in: false,
                    }),
                }
            }
            Request::Approval { place } => self.approval(&place).await,
            Request::Approve {
                chat,
                request,
                place,
                runs_in,
                keep_going,
            } => {
                let asked = self.event(&chat, request)?;
                let Body::Message(message) = &asked.body else {
                    anyhow::bail!("that is not a request");
                };
                let text = message.text.clone();
                let asker = asked.author.clone();
                self.start_run(runs::Order {
                    request: asked,
                    asker,
                    text,
                    place,
                    runs_in,
                    keep_going,
                })
                .await?;
                Ok(Response::Done)
            }
            Request::Decline {
                chat,
                request,
                place,
            } => {
                let asked = self.event(&chat, request)?;
                let thread = asked.thread.unwrap_or(asked.id);
                self.post(
                    &chat,
                    Some(thread),
                    Body::RunDeclined(player_two_core::event::RunDeclined { request, place }),
                )
                .await?;
                Ok(Response::Done)
            }
            Request::Withdraw { chat, request } => {
                let asked = self.event(&chat, request)?;
                let thread = asked.thread.unwrap_or(asked.id);
                self.post(
                    &chat,
                    Some(thread),
                    Body::RunWithdrawn(player_two_core::event::RunWithdrawn { request }),
                )
                .await?;
                Ok(Response::Done)
            }
            Request::SetMachineRunsIn { runs_in } => {
                let mut settings = self.settings()?;
                settings.machine_runs_in = runs_in;
                self.save_settings(&settings).await?;
                Ok(Response::Done)
            }
            Request::LeaveChannel { name } => {
                let name = ChannelName::new(name)?;
                self.hub()?.membership(&name, false).await?;
                self.channels_changed(None).await?;
                Ok(Response::Done)
            }
        }
    }

    /// The person's settings, or the defaults until they change one.
    fn settings(&self) -> anyhow::Result<Settings> {
        Ok(self.inner.files.read("settings.json")?.unwrap_or_default())
    }

    /// Keeps the settings, and tells the hub the places in them that can run.
    async fn save_settings(&self, settings: &Settings) -> anyhow::Result<()> {
        self.inner.files.write("settings.json", settings)?;
        self.advertise().await?;
        self.tell(ui::Event::Overview);
        Ok(())
    }

    /// Shares a folder, named after its remote when it is a git checkout.
    async fn add_folder(&self, path: String, runs_in: RunsIn) -> anyhow::Result<()> {
        if let RunsIn::Wsl(distro) = &runs_in {
            anyhow::ensure!(
                !system::is_docker_desktop(distro),
                "{distro} is Docker Desktop's own distro, which holds nobody's work"
            );
        }
        anyhow::ensure!(
            places::exists(&path, &runs_in).await,
            "there is no folder at {path}"
        );
        let mut settings = self.settings()?;
        anyhow::ensure!(
            !settings
                .folders
                .iter()
                .any(|folder| folder.path == path && folder.runs_in == runs_in),
            "{path} is shared already"
        );
        let remote = places::remote_of(&path, &runs_in).await;
        let name = places::name_for(&path, remote.as_deref(), &settings.taken())?;
        settings.folders.push(Folder {
            name,
            path,
            runs_in: runs_in.clone(),
            former: Vec::new(),
        });
        self.save_settings(&settings).await?;
        // A folder in a distro not looked in yet is advertised once its agent is found.
        self.look(Where::At(runs_in)).await?;
        Ok(())
    }

    /// One event of a chat, as the desk has it.
    fn event(&self, chat: &ChatId, id: EventId) -> anyhow::Result<Event> {
        let state = self.state();
        let chats = state
            .chats
            .as_ref()
            .context("the chats have not loaded yet")?;
        chats
            .event(chat, id)
            .cloned()
            .context("that message is not here")
    }

    /// What the approval card for a place needs.
    async fn approval(
        &self,
        place: &player_two_core::names::PlaceName,
    ) -> anyhow::Result<Response> {
        let settings = self.settings()?;
        if place.is_machine() {
            let found = self.look(Where::Everywhere).await?.agents;
            let mut choices: Vec<RunsIn> = Vec::new();
            for agent in &found {
                if !choices.contains(&agent.runs_in) {
                    choices.push(agent.runs_in.clone());
                }
            }
            let runs_in = if choices.contains(&settings.machine_runs_in) {
                settings.machine_runs_in
            } else {
                choices.first().cloned().unwrap_or(RunsIn::Windows)
            };
            return Ok(Response::Approval {
                choices,
                runs_in,
                dirty: false,
            });
        }
        let folder = settings
            .folder(place)
            .context("that place is not shared any more")?;
        self.look(Where::At(folder.runs_in.clone())).await?;
        let dirty = places::dirty(&folder.path, &folder.runs_in).await;
        Ok(Response::Approval {
            choices: vec![folder.runs_in.clone()],
            runs_in: folder.runs_in.clone(),
            dirty,
        })
    }

    /// Runs a teammate's tag of the person's place at once when it is in a thread where the
    /// place's session is kept and the person let its author keep going there.
    async fn keep_going(&self, event: &Event) {
        let Some(me) = self.me() else {
            return;
        };
        let (Body::Message(message), Some(thread)) = (&event.body, event.thread) else {
            return;
        };
        if event.author == me {
            // The person's own tags run at once as they send them.
            return;
        }
        for tag in message.tags.iter().filter(|tag| tag.person == me) {
            let Some(runs_in) = self.keeps_going(thread, &tag.place, &event.author) else {
                continue;
            };
            if self.busy(&tag.place) {
                continue;
            }
            let order = runs::Order {
                request: event.clone(),
                asker: event.author.clone(),
                text: message.text.clone(),
                place: tag.place.clone(),
                runs_in,
                keep_going: true,
            };
            if let Err(error) = self.start_run(order).await {
                eprintln!("error: cannot run a follow-up: {error:#}");
            }
        }
    }

    /// The address of the hub this machine joined, if it has.
    pub fn hub_address(&self) -> anyhow::Result<Option<String>> {
        Ok(self.inner.files.identity()?.map(|identity| identity.hub))
    }

    fn hub(&self) -> anyhow::Result<Hub> {
        self.state()
            .hub
            .clone()
            .context("this machine has not joined a hub")
    }

    fn overview(&self) -> Overview {
        let state = self.state();
        let chats = state.chats.as_ref();
        Overview {
            me: chats.map(|chats| chats.me().clone()),
            connection: state.connection.clone(),
            chats: chats.map(Chats::summaries).unwrap_or_default(),
            people: chats
                .map(|chats| chats.people().to_vec())
                .unwrap_or_default(),
            channels: chats
                .map(|chats| chats.channels().to_vec())
                .unwrap_or_default(),
            busy: self.runs_going_at(),
        }
    }

    async fn join(&self, code: &str, name: &str) -> anyhow::Result<()> {
        let code = JoinCode::decode(code)?;
        let name = PersonName::new(name.trim()).context(
            "a name is a lowercase letter, then lowercase letters, digits or -, up to 32 in all",
        )?;
        let token = hub::join(&self.inner.http, &code.hub, &code.token, &name).await?;
        self.inner.files.save_identity(&Identity {
            hub: code.hub,
            name,
            token,
        })?;
        self.inner.wake.notify_one();
        Ok(())
    }

    async fn older(&self, chat: &ChatId) -> anyhow::Result<()> {
        let hub = self.hub()?;
        let before = self
            .state()
            .chats
            .as_ref()
            .and_then(|chats| chats.oldest(chat));
        let page = hub.page(chat, before, PAGE).await?;
        let last = page.len() < usize::try_from(PAGE).unwrap_or(usize::MAX);
        if let Some(chats) = self.state().chats.as_mut() {
            chats.add_page(chat, page, last);
        }
        self.tell(ui::Event::Chat { chat: chat.clone() });
        Ok(())
    }

    /// Posts an event as the person, shows it at once, and gives back its id. The stream brings
    /// it too, which changes nothing: an event is kept by its id.
    pub(crate) async fn post(
        &self,
        chat: &ChatId,
        thread: Option<EventId>,
        body: Body,
    ) -> anyhow::Result<EventId> {
        let hub = self.hub()?;
        let posted = hub
            .post(&NewEvent {
                chat: chat.clone(),
                thread,
                body: body.clone(),
            })
            .await?;
        {
            let mut state = self.state();
            let chats = state
                .chats
                .as_mut()
                .context("the chats have not loaded yet")?;
            let event = Event {
                id: posted.id,
                chat: chat.clone(),
                thread,
                author: chats.me().clone(),
                ts: posted.ts,
                body,
            };
            chats.apply(event);
        }
        self.tell(ui::Event::Chat { chat: chat.clone() });
        self.tell(ui::Event::Overview);
        Ok(posted.id)
    }

    /// Who this machine joined as, once the chats have loaded.
    pub(crate) fn me(&self) -> Option<PersonName> {
        self.state().chats.as_ref().map(|chats| chats.me().clone())
    }

    /// Sends a message, and starts a run at each of the person's own places it tags.
    async fn send(
        &self,
        chat: ChatId,
        thread: Option<EventId>,
        text: String,
    ) -> anyhow::Result<()> {
        let text = text.trim_end().to_owned();
        anyhow::ensure!(!text.trim().is_empty(), "there is nothing to send");
        let (me, tags) = {
            let state = self.state();
            let chats = state
                .chats
                .as_ref()
                .context("the chats have not loaded yet")?;
            (chats.me().clone(), tags::read(&text, chats.people()))
        };
        let settings = self.settings()?;
        let body = Body::Message(Message {
            text: text.clone(),
            tags: tags.clone(),
        });
        let id = self.post(&chat, thread, body.clone()).await?;
        let request = Event {
            id,
            chat,
            thread,
            author: me.clone(),
            ts: 0,
            body,
        };
        // Only a tag reaches an agent: a reply in a thread without one is a comment.
        for tag in tags.into_iter().filter(|tag| tag.person == me) {
            if self.busy(&tag.place) {
                // It waits, as a teammate's request would, until the place is free.
                continue;
            }
            self.run_own(&request, &text, tag.place, &settings).await?;
        }
        Ok(())
    }

    /// Runs the person's own request at one of their places.
    async fn run_own(
        &self,
        request: &Event,
        text: &str,
        place: PlaceName,
        settings: &Settings,
    ) -> anyhow::Result<()> {
        let runs_in = if place.is_machine() {
            settings.machine_runs_in.clone()
        } else {
            settings
                .folder(&place)
                .map_or(RunsIn::Windows, |folder| folder.runs_in.clone())
        };
        self.start_run(runs::Order {
            request: request.clone(),
            asker: request.author.clone(),
            text: text.to_owned(),
            place,
            runs_in,
            keep_going: false,
        })
        .await
    }

    /// Runs a request of the person's own that waited while its place was busy.
    async fn run_now(
        &self,
        chat: &ChatId,
        request: EventId,
        place: PlaceName,
    ) -> anyhow::Result<()> {
        let asked = self.event(chat, request)?;
        let me = self.me().context("the chats have not loaded yet")?;
        let Body::Message(message) = &asked.body else {
            anyhow::bail!("that is not a request");
        };
        let waiting = self
            .state()
            .chats
            .as_ref()
            .map(|chats| chats.unanswered(chat, request))
            .unwrap_or_default();
        anyhow::ensure!(
            asked.author == me
                && waiting
                    .iter()
                    .any(|tag| tag.person == me && tag.place == place),
            "that is not a request of yours still waiting"
        );
        anyhow::ensure!(!self.busy(&place), "{place} is still busy");
        let text = message.text.clone();
        self.run_own(&asked, &text, place, &self.settings()?).await
    }

    fn read(&self, chat: &ChatId) -> anyhow::Result<()> {
        let marks = {
            let mut state = self.state();
            let Some(chats) = state.chats.as_mut() else {
                return Ok(());
            };
            chats.mark_read(chat);
            chats.read_marks().clone()
        };
        self.inner.files.write("read.json", &marks)?;
        self.tell(ui::Event::Overview);
        Ok(())
    }

    /// Fetches the channel list again, and the first page of a chat just joined.
    async fn channels_changed(&self, joined: Option<ChatId>) -> anyhow::Result<()> {
        let hub = self.hub()?;
        let channels = hub.channels().await?;
        let page = match &joined {
            Some(chat) => Some(hub.page(chat, None, PAGE).await?),
            None => None,
        };
        if let Some(chats) = self.state().chats.as_mut() {
            chats.set_channels(channels);
            if let (Some(chat), Some(page)) = (joined, page) {
                let last = page.len() < usize::try_from(PAGE).unwrap_or(usize::MAX);
                chats.add_page(&chat, page, last);
            }
        }
        self.tell(ui::Event::Overview);
        Ok(())
    }

    /// Ends the runs this desk started that a restart cut off, so the chat does not show them
    /// working for ever.
    async fn end_unfinished(&self) {
        let unfinished = self
            .state()
            .chats
            .as_ref()
            .map(Chats::unfinished)
            .unwrap_or_default();
        for (chat, thread, run) in unfinished {
            if self.runs_going(run) {
                continue;
            }
            let ended = Body::RunEnded(player_two_core::event::RunEnded {
                run,
                how: player_two_core::event::Ending::Stopped,
                reason: Some("Player Two stopped while it ran".to_owned()),
            });
            if let Err(error) = self.post(&chat, Some(thread), ended).await {
                eprintln!("error: cannot end a run cut off by a restart: {error:#}");
            }
        }
    }

    /// Fetches the people and channel lists again.
    async fn refresh(&self, hub: &Hub) -> anyhow::Result<()> {
        let (channels, people) = (hub.channels().await?, hub.people().await?);
        if let Some(chats) = self.state().chats.as_mut() {
            chats.set_channels(channels);
            chats.set_people(people);
        }
        Ok(())
    }

    /// Keeps the desk connected to the hub, for as long as the app runs.
    pub async fn run(&self) {
        let reaper = self.clone();
        tokio::spawn(async move {
            let mut minute = tokio::time::interval(Duration::from_secs(60));
            loop {
                minute.tick().await;
                reaper.forget_quiet().await;
            }
        });
        let mut delay = Duration::from_secs(1);
        loop {
            let identity = match self.inner.files.identity() {
                Ok(Some(identity)) => identity,
                Ok(None) => {
                    self.set_connection(Connection::NotJoined);
                    self.inner.wake.notified().await;
                    continue;
                }
                Err(error) => {
                    self.set_connection(Connection::Offline {
                        reason: format!("{error:#}"),
                    });
                    self.inner.wake.notified().await;
                    continue;
                }
            };
            self.set_connection(Connection::Connecting);
            let ended = self.session(&identity, &mut delay).await;
            match ended {
                Err(error) if error.downcast_ref::<NotJoined>().is_some() => {
                    // The token is dead: the person must join again, so the first-run page shows.
                    let _ = self.inner.files.forget_identity();
                    let mut state = self.state();
                    state.hub = None;
                    state.chats = None;
                    drop(state);
                    continue;
                }
                Err(error) if error.downcast_ref::<WrongVersion>().is_some() => {
                    let hub = error
                        .downcast_ref::<WrongVersion>()
                        .map(|wrong| wrong.0.clone())
                        .unwrap_or_default();
                    self.set_connection(Connection::WrongVersion { hub });
                    // Nothing else happens until the versions match: an update, or a new hub.
                    tokio::select! {
                        () = tokio::time::sleep(RECHECK) => {}
                        () = self.inner.wake.notified() => {}
                    }
                    continue;
                }
                Err(error) => self.set_connection(Connection::Offline {
                    reason: format!("{error:#}"),
                }),
                Ok(()) => self.set_connection(Connection::Offline {
                    reason: "the hub ended the stream".to_owned(),
                }),
            }
            tokio::select! {
                () = tokio::time::sleep(delay) => {}
                () = self.inner.wake.notified() => {}
            }
            delay = (delay * 2).min(MOST_DELAY);
        }
    }

    /// One connection: load what is missing, then follow the stream until it ends.
    async fn session(&self, identity: &Identity, delay: &mut Duration) -> anyhow::Result<()> {
        let hub = Hub::new(self.inner.http.clone(), identity);
        let version = hub.version().await?;
        if version != env!("CARGO_PKG_VERSION") {
            return Err(WrongVersion(version).into());
        }
        {
            let mut state = self.state();
            state.hub = Some(hub.clone());
            state.advertised = None;
        }
        // The first start looks everywhere once; after that, only Windows at start.
        let systems = if self.has_looked() {
            Where::Here
        } else {
            Where::Everywhere
        };
        self.look(systems).await?;
        let (channels, people) = (hub.channels().await?, hub.people().await?);
        {
            let mut state = self.state();
            let read: BTreeMap<ChatId, EventId> =
                self.inner.files.read("read.json")?.unwrap_or_default();
            let chats = state
                .chats
                .get_or_insert_with(|| Chats::new(identity.name.clone(), read));
            chats.set_channels(channels);
            chats.set_people(people);
        }
        let unloaded: Vec<ChatId> = {
            let state = self.state();
            let chats = state.chats.as_ref().context("the chats were just made")?;
            chats
                .mine()
                .into_iter()
                .filter(|chat| !chats.has_loaded(chat))
                .collect()
        };
        for chat in unloaded {
            let page = hub.page(&chat, None, PAGE).await?;
            let last = page.len() < usize::try_from(PAGE).unwrap_or(usize::MAX);
            if let Some(chats) = self.state().chats.as_mut() {
                chats.add_page(&chat, page, last);
            }
        }
        self.end_unfinished().await;
        let after = self
            .state()
            .chats
            .as_ref()
            .and_then(Chats::last_id)
            .unwrap_or(EventId(0));
        let mut stream = hub.stream(Some(after)).await?;
        *delay = Duration::from_secs(1);
        self.set_connection(Connection::Connected);
        let mut refresh = tokio::time::interval(REFRESH);
        refresh.tick().await;
        loop {
            tokio::select! {
                incoming = stream.next() => match incoming? {
                    Some(Incoming::Event(event)) => {
                        let chat = event.chat.clone();
                        self.asked_to_stop(&event);
                        let follow_up = event.clone();
                        let (thread, author) = (event.thread, event.author.clone());
                        let notice = self.state().chats.as_mut().and_then(|chats| {
                            let notice = chats.notice(&event);
                            chats.apply(event);
                            notice
                        });
                        self.tell(ui::Event::Chat { chat: chat.clone() });
                        self.tell(ui::Event::Overview);
                        // A follow-up that keep going runs at once has nothing to click.
                        let runs_by_itself = notice.as_ref().zip(thread).is_some_and(|(notice, thread)| {
                            notice.follows.iter().any(|place| {
                                self.keeps_going(thread, place, &author).is_some() && !self.busy(place)
                            })
                        });
                        if let Some(notice) = notice.filter(|_| !runs_by_itself) {
                            self.tell(ui::Event::Notify {
                                chat,
                                thread: notice.thread,
                                title: notice.title,
                                body: notice.body,
                            });
                        }
                        self.keep_going(&follow_up).await;
                    }
                    Some(Incoming::Presence(presence)) => {
                        if let Some(chats) = self.state().chats.as_mut() {
                            chats.presence(&presence);
                        }
                        // Someone arriving may be new, or may bring new places or channels.
                        if presence.here {
                            self.refresh(&hub).await?;
                        }
                        self.tell(ui::Event::Overview);
                    }
                    None => return Ok(()),
                },
                _ = refresh.tick() => {
                    self.refresh(&hub).await?;
                    self.tell(ui::Event::Overview);
                }
            }
        }
    }
}

/// Starts an agent and opens a session, to see that it is installed and signed in.
async fn check(agent: &agents::Found) -> Response {
    let (said, _heard) = tokio::sync::mpsc::unbounded_channel();
    match acp::Session::open(agent, &agent.home, said).await {
        Ok(session) => {
            let _ = session.stop().await;
            Response::Checked {
                ready: true,
                message: format!("{} is ready", agent.kind.label()),
                sign_in: false,
            }
        }
        Err(error) => Response::Checked {
            ready: false,
            message: format!("{error:#}"),
            sign_in: error.downcast_ref::<acp::NeedsSignIn>().is_some(),
        },
    }
}

/// The name to offer at first run, from the system's user name.
fn suggested_name() -> Option<PersonName> {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .ok()?;
    PersonName::from_user_name(&user)
}
