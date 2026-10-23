//! The hub's HTTP API: its state, its routes, who is asking, and how it refuses.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, FromRequestParts, Path, Query, Request, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use player_two_core::api::{
    ApiError, ChannelInfo, JoinCodeReply, JoinRequest, Joined, NewChannel, NewEvent, Person,
    PlaceInfo, PlaceKind, Posted, ResetRequest, VersionInfo,
};
use player_two_core::event::{Event, EventId};
use player_two_core::join_code::JoinCode;
use player_two_core::names::{ChannelName, ChatId, PersonName};
use rusqlite::OptionalExtension;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::clock::Clock;
use crate::db::Db;
use crate::release;
use crate::rules::{self, Verdict};
use crate::store;
use crate::stream::{self, Streams};

/// What the hub is told when it starts.
#[derive(Debug, Clone)]
pub struct Config {
    /// The shared join token.
    pub join_token: String,
    /// The token for `/admin/*`.
    pub admin_token: String,
    /// The hub's own address, as people reach it: for join codes and the update manifest.
    pub url: String,
    /// Where the nightly copies of the database go.
    pub backups: PathBuf,
    /// How many days events are kept.
    pub retention_days: u32,
    /// The folder holding the installer and its signature.
    pub release: PathBuf,
}

/// What every request can reach.
#[derive(Clone)]
pub struct AppState {
    /// The database.
    pub db: Db,
    /// The settings it started with.
    pub config: Arc<Config>,
    /// The time.
    pub clock: Clock,
    /// The open streams.
    pub streams: Arc<Streams>,
}

/// All of the hub's routes.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/version", get(version))
        .route("/download", get(release::download))
        .route("/update", get(release::update))
        .route("/join", post(join))
        .route("/channels", get(list_channels).post(create_channel))
        .route("/channels/{name}/join", post(join_channel))
        .route("/channels/{name}/leave", post(leave_channel))
        .route("/chats/{chat}/events", get(chat_events))
        .route("/events", post(post_event))
        .route("/stream", get(stream::stream))
        .route("/places", put(set_places))
        .route("/people", get(people))
        .route("/admin/join-code", post(admin_join_code))
        .route("/admin/reset", post(admin_reset))
        .with_state(state)
}

/// A refusal: a status and words a person can act on.
#[derive(Debug)]
pub struct Refusal {
    status: StatusCode,
    error: String,
}

impl Refusal {
    /// A refusal with this status and message.
    pub fn new(status: StatusCode, error: impl Into<String>) -> Self {
        Self {
            status,
            error: error.into(),
        }
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        (self.status, Json(ApiError { error: self.error })).into_response()
    }
}

impl From<anyhow::Error> for Refusal {
    fn from(error: anyhow::Error) -> Self {
        // The detail is for whoever runs the hub; the caller learns only that it failed.
        eprintln!("error: {error:#}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the hub failed; whoever runs it can see why in its log",
        )
    }
}

/// What a route gives back: its answer, or a refusal.
pub type Reply<T> = Result<T, Refusal>;

/// A JSON body, refused with the hub's own error shape when it does not parse.
#[derive(Debug, Clone)]
pub struct Input<T>(pub T);

impl<T, S> FromRequest<S> for Input<T>
where
    Json<T>: FromRequest<S, Rejection = JsonRejection>,
    S: Send + Sync,
{
    type Rejection = Refusal;

    async fn from_request(request: Request, state: &S) -> Reply<Self> {
        match Json::<T>::from_request(request, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(Refusal::new(StatusCode::BAD_REQUEST, rejection.body_text())),
        }
    }
}

/// The SHA-256 of a token, in hex, as the database keeps it.
fn digest(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Whether `given` is `expected`, taking as long whatever the answer.
fn same_token(given: &str, expected: &str) -> bool {
    let (given, expected) = (Sha256::digest(given), Sha256::digest(expected));
    given
        .iter()
        .zip(expected.iter())
        .fold(0_u8, |differ, (a, b)| differ | (a ^ b))
        == 0
}

/// A new personal token: 32 random bytes in hex.
fn new_token() -> anyhow::Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| anyhow::anyhow!("no randomness: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// The bearer token on a request, if it has one.
fn bearer(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

/// The person a request is from, by their personal token.
#[derive(Debug, Clone)]
pub struct Caller(pub PersonName);

impl FromRequestParts<AppState> for Caller {
    type Rejection = Refusal;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Reply<Self> {
        let refused = || {
            Refusal::new(
                StatusCode::UNAUTHORIZED,
                "this machine is not joined to the hub; join again with a join code",
            )
        };
        let token = digest(bearer(parts).ok_or_else(refused)?);
        let name: Option<String> = state
            .db
            .call(move |connection| {
                Ok(connection
                    .query_row(
                        "SELECT name FROM people WHERE token_sha256 = ?1",
                        [&token],
                        |row| row.get(0),
                    )
                    .optional()?)
            })
            .await?;
        let name = name.ok_or_else(refused)?;
        Ok(Self(PersonName::new(name).map_err(anyhow::Error::from)?))
    }
}

/// `POST /join`.
async fn join(
    State(state): State<AppState>,
    Input(request): Input<JoinRequest>,
) -> Reply<Json<Joined>> {
    if !same_token(&request.join_token, &state.config.join_token) {
        return Err(Refusal::new(
            StatusCode::FORBIDDEN,
            "that join token is not this hub's; ask for a new join code",
        ));
    }
    let token = new_token()?;
    let digest = digest(&token);
    let now = state.clock.now();
    let name = request.name;
    let joined = state
        .db
        .call(move |connection| {
            // A name whose token was dropped by `reset` may be joined again; any other is taken.
            let changed = connection.execute(
                "INSERT INTO people (name, token_sha256, joined) VALUES (?1, ?2, ?3)
                 ON CONFLICT (name) DO UPDATE SET token_sha256 = excluded.token_sha256
                 WHERE people.token_sha256 IS NULL",
                (name.as_str(), &digest, now),
            )?;
            Ok(changed == 1)
        })
        .await?;
    if joined {
        Ok(Json(Joined { token }))
    } else {
        Err(Refusal::new(
            StatusCode::CONFLICT,
            "that name is taken; if it is yours, an admin can run `player-two-hub reset NAME` to let you join again",
        ))
    }
}

/// A channel name from a path, refused like any other bad input.
fn channel_name(name: String) -> Reply<ChannelName> {
    ChannelName::new(name).map_err(|error| Refusal::new(StatusCode::BAD_REQUEST, error.to_string()))
}

/// `GET /channels`.
async fn list_channels(State(state): State<AppState>, _: Caller) -> Reply<Json<Vec<ChannelInfo>>> {
    Ok(Json(
        state
            .db
            .call(|connection| store::channels(connection))
            .await?,
    ))
}

/// `POST /channels`.
async fn create_channel(
    State(state): State<AppState>,
    Caller(me): Caller,
    Input(request): Input<NewChannel>,
) -> Reply<StatusCode> {
    let now = state.clock.now();
    let name = request.name;
    let made = state
        .db
        .call(move |connection| store::create_channel(connection, &name, &me, now))
        .await?;
    if made {
        Ok(StatusCode::CREATED)
    } else {
        Err(Refusal::new(
            StatusCode::CONFLICT,
            "there is already a channel with that name; join it instead",
        ))
    }
}

/// `POST /channels/{name}/join`.
async fn join_channel(
    State(state): State<AppState>,
    Caller(me): Caller,
    Path(name): Path<String>,
) -> Reply<StatusCode> {
    let name = channel_name(name)?;
    let joined = state
        .db
        .call(move |connection| store::join_channel(connection, &name, &me))
        .await?;
    if joined {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(Refusal::new(
            StatusCode::NOT_FOUND,
            "there is no channel with that name",
        ))
    }
}

/// `POST /channels/{name}/leave`.
async fn leave_channel(
    State(state): State<AppState>,
    Caller(me): Caller,
    Path(name): Path<String>,
) -> Reply<StatusCode> {
    let name = channel_name(name)?;
    state
        .db
        .call(move |connection| store::leave_channel(connection, &name, &me))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The most events one page may hold, and how many a page holds when the reader does not say.
const PAGE_MOST: u32 = 200;
const PAGE_USUAL: u32 = 50;

/// `GET /chats/{chat}/events`'s query.
#[derive(Debug, Deserialize)]
struct PageQuery {
    before: Option<i64>,
    limit: Option<u32>,
}

/// `GET /chats/{chat}/events`.
async fn chat_events(
    State(state): State<AppState>,
    Caller(me): Caller,
    Path(chat): Path<String>,
    Query(query): Query<PageQuery>,
) -> Reply<Json<Vec<Event>>> {
    let chat = ChatId::try_from(chat)
        .map_err(|error| Refusal::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    let limit = query.limit.unwrap_or(PAGE_USUAL).clamp(1, PAGE_MOST);
    let before = query.before.map(EventId);
    let page = state
        .db
        .call(move |connection| {
            if store::may_use(connection, &chat, &me)? {
                Ok(Some(store::page(connection, &chat, before, limit)?))
            } else {
                Ok(None)
            }
        })
        .await?;
    page.map(Json)
        .ok_or_else(|| Refusal::new(StatusCode::FORBIDDEN, "you are not in that chat"))
}

/// `POST /events`.
async fn post_event(
    State(state): State<AppState>,
    Caller(me): Caller,
    Input(new): Input<NewEvent>,
) -> Reply<Json<Posted>> {
    let now = state.clock.now();
    let stored = state
        .db
        .call(move |connection| {
            let transaction = connection.transaction()?;
            if let Verdict::Refused(reason) = rules::may_post(&transaction, &new, &me)? {
                return Ok(Err(reason));
            }
            let event =
                store::insert_event(&transaction, &new.chat, new.thread, &me, &new.body, now)?;
            transaction.commit()?;
            Ok(Ok(event))
        })
        .await?;
    match stored {
        Ok(event) => {
            let posted = Posted {
                id: event.id,
                ts: event.ts,
            };
            state.streams.publish(event);
            Ok(Json(posted))
        }
        Err(reason) => Err(Refusal::new(StatusCode::FORBIDDEN, reason)),
    }
}

/// The most places one person may advertise.
const MOST_PLACES: usize = 200;

/// `PUT /places`.
async fn set_places(
    State(state): State<AppState>,
    Caller(me): Caller,
    Input(places): Input<Vec<PlaceInfo>>,
) -> Reply<StatusCode> {
    if places.len() > MOST_PLACES {
        return Err(Refusal::new(
            StatusCode::BAD_REQUEST,
            format!("at most {MOST_PLACES} places"),
        ));
    }
    // A machine is called `machine`, and nothing else is.
    if places
        .iter()
        .any(|place| place.name.is_machine() != (place.kind == PlaceKind::Machine))
    {
        return Err(Refusal::new(
            StatusCode::BAD_REQUEST,
            "only the machine may be called machine",
        ));
    }
    state
        .db
        .call(move |connection| store::set_places(connection, &me, &places))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /people`.
async fn people(State(state): State<AppState>, _: Caller) -> Reply<Json<Vec<Person>>> {
    let people = state
        .db
        .call(|connection| store::people(connection))
        .await?;
    Ok(Json(
        people
            .into_iter()
            .map(|(name, last_seen, places)| Person {
                here: state.streams.is_here(&name),
                name,
                last_seen,
                places,
            })
            .collect(),
    ))
}

/// A request made with the admin token.
#[derive(Debug, Clone, Copy)]
pub struct Admin;

impl FromRequestParts<AppState> for Admin {
    type Rejection = Refusal;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Reply<Self> {
        match bearer(parts) {
            Some(token) if same_token(token, &state.config.admin_token) => Ok(Self),
            _ => Err(Refusal::new(
                StatusCode::UNAUTHORIZED,
                "that is not the hub's admin token",
            )),
        }
    }
}

/// `POST /admin/join-code`.
async fn admin_join_code(State(state): State<AppState>, _: Admin) -> Json<JoinCodeReply> {
    let code = JoinCode {
        hub: state.config.url.clone(),
        token: state.config.join_token.clone(),
    };
    Json(JoinCodeReply {
        code: code.encode(),
    })
}

/// `POST /admin/reset`.
async fn admin_reset(
    State(state): State<AppState>,
    _: Admin,
    Input(request): Input<ResetRequest>,
) -> Reply<StatusCode> {
    let name = request.name;
    let who = name.clone();
    let changed = state
        .db
        .call(move |connection| {
            Ok(connection.execute(
                "UPDATE people SET token_sha256 = NULL WHERE name = ?1",
                [who.as_str()],
            )?)
        })
        .await?;
    if changed == 0 {
        return Err(Refusal::new(
            StatusCode::NOT_FOUND,
            "nobody of that name has joined",
        ));
    }
    state.streams.close(&name);
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /healthz`: answers once the database answers.
async fn healthz(State(state): State<AppState>) -> Reply<&'static str> {
    state
        .db
        .call(|connection| Ok(connection.query_row("SELECT 1", [], |_| Ok(()))?))
        .await?;
    Ok("ok")
}

/// `GET /version`.
async fn version() -> Json<VersionInfo> {
    Json(VersionInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}
