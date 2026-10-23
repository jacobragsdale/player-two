//! Phase 5's scenario: Jacob asks Alice's machine for help, Alice approves at ask mode and hands
//! Jacob the steps, a follow-up runs under keep going, Alice stops, a later follow-up waits for
//! her, and Carol, in the chat, sees it all but cannot answer a step.

mod common;

use common::{Hub, chat};
use player_two_core::event::{Mode, RunsIn, StepsBy};
use player_two_core::names::{ChatId, PlaceName};
use player_two_desk::Desk;
use player_two_desk::files::Files;
use player_two_desk::places::Settings;
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
            mode: None,
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
    let settings = Settings {
        custom_agent: Some(vec![env!("CARGO_BIN_EXE_fake-acp-agent").to_owned()]),
        ..Settings::default()
    };
    Files::new(hub.folder("alice"))
        .write("settings.json", &settings)
        .expect("written");
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
    assert_eq!(
        alice
            .handle(Request::SetMachineRunsIn {
                runs_in: RunsIn::Windows
            })
            .await,
        Ok(Response::Done)
    );
    common::until(&jacob, "alice's places", |overview| {
        overview
            .people
            .iter()
            .any(|person| person.name.as_str() == "alice" && !person.places.is_empty())
    })
    .await;
    (alice, jacob, carol)
}

fn approve(request: &Item, steps_by: StepsBy, keep_going: bool) -> Request {
    Request::Approve {
        chat: id(CHAT),
        request: request.id,
        place: PlaceName::machine(),
        mode: Mode::Ask,
        runs_in: RunsIn::Windows,
        steps_by,
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
    assert_eq!(request.waiting[0].person.as_str(), "alice");
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

    // Alice approves, handing the steps to Jacob and letting him keep going.
    assert_eq!(
        alice.handle(approve(&request, StepsBy::Asker, true)).await,
        Ok(Response::Done)
    );
    let asked = until(&jacob, None, ask, "the first step to reach jacob", |item| {
        item.runs.first().is_some_and(|run| run.prompts.len() == 1)
    })
    .await;
    let run = asked.runs[0].clone();
    assert!(
        run.may_answer && !run.mine,
        "jacob answers the steps but does not own the run"
    );
    let seen_by_carol = until(&carol, None, ask, "carol to see the step", |item| {
        item.runs.first().is_some_and(|run| run.prompts.len() == 1)
    })
    .await;
    assert!(!seen_by_carol.runs[0].may_answer, "carol may not answer");
    let refused = carol
        .handle(Request::Answer {
            chat: id(CHAT),
            thread: run.thread,
            run: run.id,
            prompt: run.prompts[0].id.clone(),
            allow: true,
        })
        .await;
    assert!(refused.is_err(), "the hub refuses carol's answer");

    let answer = |prompt: &str, allow: bool| Request::Answer {
        chat: id(CHAT),
        thread: run.thread,
        run: run.id,
        prompt: prompt.to_owned(),
        allow,
    };
    assert_eq!(
        jacob.handle(answer(&run.prompts[0].id, true)).await,
        Ok(Response::Done)
    );
    let second = until(&jacob, None, ask, "the second step", |item| {
        item.runs[0].prompts.len() == 2
    })
    .await;
    assert_eq!(
        jacob
            .handle(answer(&second.runs[0].prompts[1].id, false))
            .await,
        Ok(Response::Done)
    );
    let done = until(&carol, None, ask, "the run to finish", |item| {
        item.runs[0].state == RunState::Done
    })
    .await;
    assert_eq!(
        done.runs[0].result.as_deref(),
        Some("Did: read pip.conf, edit done, run refused.")
    );
    assert_eq!(
        done.runs[0].prompts[0].by.as_ref().map(|by| by.as_str()),
        Some("jacob")
    );

    // A follow-up in the thread runs at once, in the same session, under keep going.
    let thread = Some(run.thread.0);
    send(&jacob, "now read it again", thread).await;
    let again = until(
        &carol,
        thread,
        "now read it again",
        "the follow-up to run",
        |item| {
            item.runs
                .first()
                .is_some_and(|run| run.state == RunState::Done)
        },
    )
    .await;
    assert_eq!(again.runs[0].result.as_deref(), Some("Did: read pip.conf."));

    // Alice stops the next one, which ends keeping going.
    send(&jacob, "and be slow about it", thread).await;
    let slow = until(
        &alice,
        thread,
        "and be slow about it",
        "the slow follow-up",
        |item| {
            item.runs
                .first()
                .is_some_and(|run| run.state == RunState::Working)
        },
    )
    .await;
    assert_eq!(
        alice
            .handle(Request::Stop {
                run: slow.runs[0].id
            })
            .await,
        Ok(Response::Done)
    );
    until(&jacob, thread, "and be slow about it", "the stop", |item| {
        item.runs[0].state == RunState::Stopped
    })
    .await;

    // So the next follow-up waits for her, and she sees it as hers to decide.
    send(&jacob, "read it once more", thread).await;
    let waiting = until(
        &alice,
        thread,
        "read it once more",
        "the follow-up to wait",
        |item| item.follows.is_some(),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let still = until(
        &alice,
        thread,
        "read it once more",
        "it to keep waiting",
        |_| true,
    )
    .await;
    assert!(still.runs.is_empty(), "nothing ran without alice");
    let follows = waiting.follows.expect("it follows a run");
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
                request: request.id
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
