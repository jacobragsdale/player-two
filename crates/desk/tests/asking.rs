//! The help-with-my-machine scenario: Jacob asks Alice's machine for help, Alice approves once
//! and the agent asks nobody after, a comment runs nothing, a tagged follow-up runs under keep
//! going, Carol's tag goes to the same session once Alice lets it, Alice stops, and a later
//! follow-up waits for her.

mod common;

use common::{Hub, chat};
use player_two_core::event::RunsIn;
use player_two_core::names::{ChatId, PlaceName};
use player_two_desk::Desk;
use player_two_desk::ui::{ChatView, Item, Request, Response, RunState};

const CHAT: &str = "#help";

fn id(chat: &str) -> ChatId {
    ChatId::try_from(chat.to_owned()).expect("a chat id")
}

async fn send(desk: &Desk, text: &str, thread: Option<i64>) {
    let thread = thread.map(player_two_core::event::EventId);
    let sent = desk
        .handle(Request::Send {
            chat: id(CHAT),
            thread,
            text: text.to_owned(),
        })
        .await;
    assert_eq!(sent, Ok(Response::Done));
}

/// Waits up to ten seconds for the message saying `text`, in the chat or a thread, to satisfy `test`.
async fn until(
    desk: &Desk,
    thread: Option<i64>,
    text: &str,
    what: &str,
    test: impl Fn(&Item) -> bool,
) -> Item {
    for _ in 0..500 {
        let view: ChatView = chat(desk, CHAT, thread.map(player_two_core::event::EventId)).await;
        if let Some(item) = view.items.iter().find(|item| item.text == text)
            && test(item)
        {
            return item.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!(
        "waited too long for {what}: {:?}",
        chat(desk, CHAT, thread.map(player_two_core::event::EventId))
            .await
            .items
            .iter()
            .find(|item| item.text == text)
    );
}

/// Alice, Jacob and Carol in #help; Alice's agent is the fake one.
async fn three(hub: &Hub) -> (Desk, Desk, Desk) {
    let alice = hub.joined("alice").await;
    let jacob = hub.joined("jacob").await;
    let carol = hub.joined("carol").await;
    common::fake_agent(hub, &alice, "alice").await;
    assert_eq!(
        alice
            .handle(Request::CreateChannel {
                name: "help".to_owned()
            })
            .await,
        Ok(Response::Done)
    );
    for desk in [&jacob, &carol] {
        assert_eq!(
            desk.handle(Request::JoinChannel {
                name: "help".to_owned()
            })
            .await,
            Ok(Response::Done)
        );
    }
    // Alice's machine is advertised, so a tag of her reads as one.
    common::until(&jacob, "alice's places", |overview| {
        overview
            .people
            .iter()
            .any(|person| person.name.as_str() == "alice" && !person.places.is_empty())
    })
    .await;
    (alice, jacob, carol)
}

fn approve(request: &Item, keep_going: bool) -> Request {
    Request::Approve {
        chat: id(CHAT),
        request: request.id,
        place: PlaceName::machine(),
        runs_in: RunsIn::Windows,
        keep_going,
    }
}

#[tokio::test]
async fn jacob_helps_alice_fix_her_proxy() {
    let hub = Hub::start().await;
    let (alice, jacob, carol) = three(&hub).await;

    let ask = "@alice can your agent read the pip settings, then edit and run the fix?";
    send(&jacob, ask, None).await;
    let request = until(&alice, None, ask, "the request to reach alice", |item| {
        item.waiting.len() == 1
    })
    .await;
    assert_eq!(request.waiting[0].tag.person.as_str(), "alice");
    let Ok(Response::Approval {
        choices,
        runs_in,
        dirty,
    }) = alice
        .handle(Request::Approval {
            place: PlaceName::machine(),
        })
        .await
    else {
        panic!("the approval card has what it needs");
    };
    assert_eq!(
        (choices, runs_in, dirty),
        (vec![RunsIn::Windows], RunsIn::Windows, false)
    );

    // Alice approves once, letting Jacob keep going, and the agent asks nobody about its steps.
    assert_eq!(
        alice.handle(approve(&request, true)).await,
        Ok(Response::Done)
    );
    let done = until(&carol, None, ask, "the run to finish", |item| {
        item.runs
            .first()
            .is_some_and(|run| run.state == RunState::Done)
    })
    .await;
    let run = done.runs[0].clone();
    assert_eq!(
        run.result.as_deref(),
        Some("Did: read pip.conf, edit done, run done.")
    );
    assert!(
        run.activity.iter().any(|item| item.id == "run-1"),
        "carol sees what the agent did"
    );
    assert!(
        !run.mine && !run.may_stop,
        "carol neither owns nor asked for it"
    );

    // A comment in the thread reaches no agent.
    let thread = Some(run.thread.0);
    send(&jacob, "thanks!", thread).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let comment = until(&alice, thread, "thanks!", "the comment", |_| true).await;
    assert!(comment.runs.is_empty() && comment.waiting.is_empty());

    // A follow-up tags the place again, and runs at once in the same session, under keep going.
    let again = "@alice now echo and read it again";
    send(&jacob, again, thread).await;
    let ran = until(&carol, thread, again, "the follow-up to run", |item| {
        item.runs
            .first()
            .is_some_and(|run| run.state == RunState::Done)
    })
    .await;
    let answer = ran.runs[0].result.clone().unwrap_or_default();
    assert!(answer.starts_with("Did: read pip.conf."), "{answer}");
    assert!(
        !answer.contains("Earlier in this conversation"),
        "the kept session needs no retelling"
    );

    // Carol's tag of the place goes to the same session, but only once Alice lets it.
    let hers = "@alice read it for me too";
    send(&carol, hers, thread).await;
    let asked = until(&alice, thread, hers, "carol's tag to wait", |item| {
        item.waiting.len() == 1
    })
    .await;
    assert!(
        asked.waiting[0].follows.is_some(),
        "alice sees it follows the run"
    );
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(
        until(&alice, thread, hers, "it to keep waiting", |_| true)
            .await
            .runs
            .is_empty(),
        "keep going was for jacob, not carol"
    );
    assert_eq!(
        alice.handle(approve(&asked, false)).await,
        Ok(Response::Done)
    );
    let ran = until(&carol, thread, hers, "carol's run", |item| {
        item.runs
            .first()
            .is_some_and(|run| run.state == RunState::Done)
    })
    .await;
    assert_eq!(ran.runs[0].result.as_deref(), Some("Did: read pip.conf."));

    // Jacob still keeps going; Alice stops his next one, which ends it.
    let slow = "@alice and be slow about it";
    send(&jacob, slow, thread).await;
    let working = until(&alice, thread, slow, "the slow follow-up", |item| {
        item.runs
            .first()
            .is_some_and(|run| run.state == RunState::Working)
    })
    .await;
    assert_eq!(
        alice
            .handle(Request::Stop {
                chat: id(CHAT),
                thread: working.runs[0].thread,
                run: working.runs[0].id
            })
            .await,
        Ok(Response::Done)
    );
    until(&jacob, thread, slow, "the stop", |item| {
        item.runs[0].state == RunState::Stopped
    })
    .await;

    // So the next follow-up waits for her, and she sees it as hers to decide.
    let more = "@alice read it once more";
    send(&jacob, more, thread).await;
    let waiting = until(&alice, thread, more, "the follow-up to wait", |item| {
        item.waiting.len() == 1
    })
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let still = until(&alice, thread, more, "it to keep waiting", |_| true).await;
    assert!(still.runs.is_empty(), "nothing ran without alice");
    let follows = waiting.waiting[0]
        .follows
        .clone()
        .expect("it follows a run");
    assert_eq!(
        (follows.owner.as_str(), follows.place.as_str()),
        ("alice", "machine")
    );
}

#[tokio::test]
async fn a_request_can_be_declined_or_taken_back() {
    let hub = Hub::start().await;
    let (alice, jacob, _carol) = three(&hub).await;

    send(&jacob, "@alice please look", None).await;
    let request = until(&alice, None, "@alice please look", "the request", |item| {
        item.waiting.len() == 1
    })
    .await;
    assert_eq!(
        alice
            .handle(Request::Decline {
                chat: id(CHAT),
                request: request.id,
                place: PlaceName::machine(),
            })
            .await,
        Ok(Response::Done)
    );
    let declined = until(&jacob, None, "@alice please look", "the decline", |item| {
        item.waiting.is_empty()
    })
    .await;
    assert!(declined.runs.is_empty());

    send(&jacob, "@alice actually never mind", None).await;
    let request = until(
        &jacob,
        None,
        "@alice actually never mind",
        "the second request",
        |item| item.waiting.len() == 1,
    )
    .await;
    let refused = alice
        .handle(Request::Withdraw {
            chat: id(CHAT),
            request: request.id,
        })
        .await;
    assert!(refused.is_err(), "only the asker takes a request back");
    assert_eq!(
        jacob
            .handle(Request::Withdraw {
                chat: id(CHAT),
                request: request.id
            })
            .await,
        Ok(Response::Done)
    );
    until(
        &alice,
        None,
        "@alice actually never mind",
        "the withdrawal",
        |item| item.waiting.is_empty(),
    )
    .await;

    send(&jacob, "@nobody and @alice/billing", None).await;
    let unknown = until(
        &jacob,
        None,
        "@nobody and @alice/billing",
        "the unknown tags",
        |item| item.unknown.len() == 2,
    )
    .await;
    assert_eq!(
        unknown.unknown,
        [
            "nobody named nobody has joined",
            "alice does not share billing"
        ]
    );
}

#[tokio::test]
async fn the_asker_can_stop_a_run_but_nobody_else() {
    let hub = Hub::start().await;
    let (alice, jacob, carol) = three(&hub).await;

    let ask = "@alice be slow about it";
    send(&jacob, ask, None).await;
    let request = until(&alice, None, ask, "the request", |item| {
        item.waiting.len() == 1
    })
    .await;
    assert_eq!(
        alice.handle(approve(&request, false)).await,
        Ok(Response::Done)
    );
    let working = until(&jacob, None, ask, "the run to start", |item| {
        item.runs
            .first()
            .is_some_and(|run| run.state == RunState::Working)
    })
    .await;
    let run = &working.runs[0];
    assert!(run.may_stop && !run.mine, "jacob asked, so he may stop it");
    assert_eq!(
        common::overview(&alice).await.running,
        ["your computer, for jacob"]
    );
    let stop = Request::Stop {
        chat: id(CHAT),
        thread: run.thread,
        run: run.id,
    };
    let seen_by_carol = until(&carol, None, ask, "carol to see it", |item| {
        !item.runs.is_empty()
    })
    .await;
    assert!(!seen_by_carol.runs[0].may_stop);
    assert!(
        carol.handle(stop.clone()).await.is_err(),
        "the hub refuses carol's stop"
    );
    assert_eq!(jacob.handle(stop).await, Ok(Response::Done));
    let stopped = until(&carol, None, ask, "the run to stop", |item| {
        item.runs[0].state == RunState::Stopped
    })
    .await;
    assert_eq!(stopped.runs[0].reason.as_deref(), Some("jacob stopped it"));
}

#[tokio::test]
async fn a_follow_up_sent_to_another_system_starts_afresh_there() {
    let hub = Hub::start().await;
    let (alice, jacob, _carol) = three(&hub).await;
    let ask = "@alice read the settings";
    send(&jacob, ask, None).await;
    let request = until(&alice, None, ask, "the request", |item| {
        item.waiting.len() == 1
    })
    .await;
    assert_eq!(
        alice.handle(approve(&request, false)).await,
        Ok(Response::Done)
    );
    until(&alice, None, ask, "the run to finish", |item| {
        item.runs
            .first()
            .is_some_and(|run| run.state == RunState::Done)
    })
    .await;

    // Alice sends the follow-up to a WSL distro, which has no agent here: the session kept on
    // Windows is not used for it.
    let thread = Some(request.id.0);
    let more = "@alice read it again";
    send(&jacob, more, thread).await;
    let asked = until(&alice, thread, more, "the follow-up", |item| {
        item.waiting.len() == 1
    })
    .await;
    let elsewhere = Request::Approve {
        chat: id(CHAT),
        request: asked.id,
        place: PlaceName::machine(),
        runs_in: RunsIn::Wsl("Ubuntu".to_owned()),
        keep_going: false,
    };
    assert_eq!(alice.handle(elsewhere).await, Ok(Response::Done));
    let failed = until(&jacob, thread, more, "the follow-up to end", |item| {
        item.runs
            .first()
            .is_some_and(|run| run.state != RunState::Working)
    })
    .await;
    assert_eq!(failed.runs[0].state, RunState::Failed);
    assert!(
        failed.runs[0]
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("Ubuntu")),
        "{:?}",
        failed.runs[0].reason
    );
}

#[tokio::test]
async fn each_tag_of_a_request_is_answered_on_its_own() {
    let hub = Hub::start().await;
    let (alice, jacob, _carol) = three(&hub).await;
    let folder = tempfile::tempdir().expect("a folder");
    let path = folder.path().join("proj");
    std::fs::create_dir(&path).expect("made");
    let picked = path.to_string_lossy().into_owned();
    alice.set_picker(std::sync::Arc::new(move || {
        let folder = picked.clone();
        Box::pin(async move { Some(folder) })
    }));
    assert_eq!(alice.handle(Request::PickFolder).await, Ok(Response::Done));
    common::until(&jacob, "proj to be advertised", |overview| {
        overview
            .people
            .iter()
            .any(|person| person.name.as_str() == "alice" && person.places.len() == 2)
    })
    .await;

    let ask = "@alice @alice/proj read it";
    send(&jacob, ask, None).await;
    let request = until(&alice, None, ask, "both tags to wait", |item| {
        item.waiting.len() == 2
    })
    .await;
    assert_eq!(
        alice
            .handle(Request::Decline {
                chat: id(CHAT),
                request: request.id,
                place: PlaceName::new("proj").expect("a name"),
            })
            .await,
        Ok(Response::Done)
    );
    let left = until(&jacob, None, ask, "the decline", |item| {
        item.waiting.len() == 1
    })
    .await;
    assert_eq!(left.waiting[0].tag.place, PlaceName::machine());
    assert_eq!(
        alice.handle(approve(&request, false)).await,
        Ok(Response::Done)
    );
    let ran = until(&jacob, None, ask, "the machine to run it", |item| {
        item.runs
            .first()
            .is_some_and(|run| run.state == RunState::Done)
    })
    .await;
    assert!(ran.waiting.is_empty());
    assert_eq!(ran.runs.len(), 1);
}

#[tokio::test]
async fn a_tag_of_someone_who_just_joined_the_channel_reaches_them() {
    let hub = Hub::start().await;
    let (alice, jacob, _carol) = three(&hub).await;
    assert_eq!(
        jacob
            .handle(Request::CreateChannel {
                name: "side".to_owned()
            })
            .await,
        Ok(Response::Done)
    );
    // Jacob's desk has not heard that alice joined since it last fetched the channels.
    assert_eq!(
        alice
            .handle(Request::JoinChannel {
                name: "side".to_owned()
            })
            .await,
        Ok(Response::Done)
    );
    let sent = jacob
        .handle(Request::Send {
            chat: id("#side"),
            thread: None,
            text: "@alice read it".to_owned(),
        })
        .await;
    assert_eq!(sent, Ok(Response::Done));
    let view = chat(&jacob, "#side", None).await;
    let item = &view.items[0];
    assert_eq!(
        item.tags.len(),
        1,
        "the tag reached alice: {:?}",
        item.unknown
    );
    assert!(item.unknown.is_empty());
}
