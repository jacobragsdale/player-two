mod common;

use common::Hub;
use player_two_hub::nightly;
use serde_json::json;

const DAY: i64 = 86_400_000;

/// Posts a message in alice and bob's chat at `day`, in `thread` if given, and gives back its id.
async fn say(hub: &Hub, token: &str, start: i64, day: i64, text: &str, thread: Option<i64>) -> i64 {
    hub.clock.set(start + day * DAY);
    let mut event = json!({"chat": "alice+bob", "kind": "message", "body": {"text": text}});
    if let Some(thread) = thread {
        event["thread"] = json!(thread);
    }
    hub.post(token, "/events", &event).await.body["id"]
        .as_i64()
        .expect("an id")
}

/// The texts in alice and bob's chat, oldest first.
async fn texts(hub: &Hub, token: &str) -> Vec<String> {
    hub.get(token, "/chats/alice+bob/events")
        .await
        .body
        .as_array()
        .expect("a list")
        .iter()
        .map(|event| event["body"]["text"].as_str().expect("text").to_owned())
        .collect()
}

#[tokio::test]
async fn old_threads_go_whole() {
    let hub = Hub::start().await;
    let (alice, bob) = (hub.join("alice").await, hub.join("bob").await);
    let start = 1_700_006_400_000 + 3 * 3_600_000; // 2023-11-15 at 03:00 UTC
    say(&hub, &alice, start, 0, "old", None).await;
    let asked = say(&hub, &alice, start, 1, "a question", None).await;
    say(&hub, &bob, start, 20, "a late answer", Some(asked)).await;
    say(&hub, &alice, start, 20, "newer", None).await;

    // On night 34, what is older than day 4 goes, but a thread with a reply on day 20 stays whole.
    hub.clock.set(start + 34 * DAY);
    nightly::run(&hub.state)
        .await
        .expect("the night's job runs");
    assert_eq!(
        texts(&hub, &bob).await,
        ["a question", "a late answer", "newer"]
    );

    // On night 51, the thread's newest event is past keeping too, and it goes with its reply.
    hub.clock.set(start + 51 * DAY);
    nightly::run(&hub.state)
        .await
        .expect("the night's job runs");
    assert!(texts(&hub, &bob).await.is_empty());
}
