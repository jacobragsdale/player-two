//! Tagging your own agent: it runs at once at your mode, its steps wait for you in ask mode, a
//! busy place leaves the next tag waiting, and Stop stops it.

mod common;

use common::{Hub, chat, overview};
use player_two_core::event::Mode;
use player_two_core::names::{ChatId, PlaceName};
use player_two_desk::Desk;
use player_two_desk::ui::{ChatView, Item, Request, Response, RunState};

const CHAT: &str = "#ops";

fn id(chat: &str) -> ChatId {
    ChatId::try_from(chat.to_owned()).expect("a chat id")
}

/// A desk joined as alice, in #ops, whose agent is the fake one.
async fn alice(hub: &Hub) -> Desk {
    let desk = hub.joined("alice").await;
    common::fake_agent(hub, &desk, "alice").await;
    assert_eq!(
        desk.handle(Request::CreateChannel {
            name: "ops".to_owned()
        })
        .await,
        Ok(Response::Done)
    );
    desk
}

async fn send(desk: &Desk, text: &str, mode: Option<Mode>) {
    let sent = desk
        .handle(Request::Send {
            chat: id(CHAT),
            thread: None,
            text: text.to_owned(),
            mode,
        })
        .await;
    assert_eq!(sent, Ok(Response::Done));
}

/// Waits up to ten seconds for the message saying `text` to satisfy `test`.
async fn until_item(desk: &Desk, text: &str, what: &str, test: impl Fn(&Item) -> bool) -> Item {
    for _ in 0..500 {
        let view: ChatView = chat(desk, CHAT, None).await;
        if let Some(item) = view.items.iter().find(|item| item.text == text)
            && test(item)
        {
            return item.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!(
        "waited too long for {what}: {:?}",
        chat(desk, CHAT, None)
            .await
            .items
            .iter()
            .find(|item| item.text == text)
    );
}

#[tokio::test]
async fn in_ask_mode_each_change_waits_for_a_yes() {
    let hub = Hub::start().await;
    let desk = alice(&hub).await;
    let text = "@alice read the proxy settings, then edit and run";
    send(&desk, text, None).await;
    let item = until_item(&desk, text, "the first step to wait", |item| {
        item.runs.first().is_some_and(|run| run.prompts.len() == 1)
    })
    .await;
    let run = &item.runs[0];
    assert_eq!(
        (run.mode, run.mine, run.may_answer),
        (Mode::Ask, true, true)
    );
    assert_eq!(run.prompts[0].what, "edited pip.conf");
    let answer = |prompt: &str, allow: bool| Request::Answer {
        chat: id(CHAT),
        thread: run.thread,
        run: run.id,
        prompt: prompt.to_owned(),
        allow,
    };
    assert_eq!(
        desk.handle(answer(&run.prompts[0].id, true)).await,
        Ok(Response::Done)
    );
    let item = until_item(&desk, text, "the second step to wait", |item| {
        item.runs[0].prompts.len() == 2
    })
    .await;
    let second = item.runs[0].prompts[1].id.clone();
    assert_eq!(
        desk.handle(answer(&second, false)).await,
        Ok(Response::Done)
    );
    let item = until_item(&desk, text, "the run to end", |item| {
        item.runs[0].state == RunState::Done
    })
    .await;
    let run = &item.runs[0];
    assert_eq!(
        run.result.as_deref(),
        Some("Did: read pip.conf, edit done, run refused.")
    );
    assert!(
        run.activity
            .iter()
            .any(|activity| activity.id == "read-1" && activity.outcome.as_deref() == Some("done"))
    );
    assert_eq!(
        run.prompts
            .iter()
            .map(|prompt| prompt.allow)
            .collect::<Vec<_>>(),
        [Some(true), Some(false)]
    );
    assert!(item.waiting.is_empty());
}

#[tokio::test]
async fn read_only_refuses_and_full_allows_without_asking() {
    let hub = Hub::start().await;
    let desk = alice(&hub).await;
    send(&desk, "@alice edit it", Some(Mode::ReadOnly)).await;
    let item = until_item(&desk, "@alice edit it", "the read-only run", |item| {
        item.runs
            .first()
            .is_some_and(|run| run.state == RunState::Done)
    })
    .await;
    assert_eq!(item.runs[0].result.as_deref(), Some("Did: edit refused."));
    assert!(item.runs[0].prompts.is_empty());
    assert_eq!(
        overview(&desk).await.default_mode,
        Mode::ReadOnly,
        "the switch became the default"
    );

    send(&desk, "@alice run it", Some(Mode::Full)).await;
    let item = until_item(&desk, "@alice run it", "the full run", |item| {
        item.runs
            .first()
            .is_some_and(|run| run.state == RunState::Done)
    })
    .await;
    assert_eq!(item.runs[0].result.as_deref(), Some("Did: run done."));
    assert_eq!(item.runs[0].mode, Mode::Full);
}

#[tokio::test]
async fn a_busy_place_leaves_the_next_tag_waiting_and_stop_stops_it() {
    let hub = Hub::start().await;
    let desk = alice(&hub).await;
    send(&desk, "@alice be slow", Some(Mode::Full)).await;
    let slow = until_item(&desk, "@alice be slow", "the slow run", |item| {
        item.runs
            .first()
            .is_some_and(|run| run.state == RunState::Working)
    })
    .await;
    send(&desk, "@alice read it", None).await;
    let waiting = until_item(&desk, "@alice read it", "the second tag", |_| true).await;
    assert!(waiting.runs.is_empty());
    assert_eq!(waiting.waiting.len(), 1, "it waits while the place is busy");
    let run_now = Request::RunNow {
        chat: id(CHAT),
        request: waiting.id,
        place: PlaceName::machine(),
    };
    assert_eq!(overview(&desk).await.busy, [PlaceName::machine()]);
    let refused = desk.handle(run_now.clone()).await;
    assert!(refused.is_err_and(|error| error.contains("still busy")));

    assert_eq!(
        desk.handle(Request::Stop {
            chat: id(CHAT),
            thread: slow.runs[0].thread,
            run: slow.runs[0].id
        })
        .await,
        Ok(Response::Done)
    );
    let stopped = until_item(&desk, "@alice be slow", "the stop", |item| {
        item.runs[0].state == RunState::Stopped
    })
    .await;
    assert!(stopped.runs[0].result.is_none());

    // Once the place is free, the waiting tag runs with one click.
    assert_eq!(desk.handle(run_now).await, Ok(Response::Done));
    let ran = until_item(&desk, "@alice read it", "the waiting tag to run", |item| {
        item.runs
            .first()
            .is_some_and(|run| run.state == RunState::Done)
    })
    .await;
    assert_eq!(ran.runs[0].result.as_deref(), Some("Did: read pip.conf."));
    assert!(ran.waiting.is_empty());
}

#[tokio::test]
async fn a_follow_up_of_your_own_run_runs_at_once_at_your_mode() {
    let hub = Hub::start().await;
    let desk = alice(&hub).await;
    send(&desk, "@alice read it", None).await;
    let first = until_item(&desk, "@alice read it", "the first run", |item| {
        item.runs
            .first()
            .is_some_and(|run| run.state == RunState::Done)
    })
    .await;
    let sent = desk
        .handle(Request::Send {
            chat: id(CHAT),
            thread: Some(first.id),
            text: "now run it".to_owned(),
            mode: Some(Mode::Full),
        })
        .await;
    assert_eq!(sent, Ok(Response::Done));
    for _ in 0..500 {
        let view = chat(&desk, CHAT, Some(first.id)).await;
        if let Some(run) = view
            .items
            .iter()
            .find(|item| item.text == "now run it")
            .and_then(|item| item.runs.first())
            && run.state == RunState::Done
        {
            assert_eq!(run.mode, Mode::Full);
            assert_eq!(run.result.as_deref(), Some("Did: run done."));
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("the follow-up did not run");
}
