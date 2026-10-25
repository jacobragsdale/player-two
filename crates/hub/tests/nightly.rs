mod common;

use common::Hub;
use player_two_hub::nightly;
use serde_json::json;

const DAY: i64 = 86_400_000;

#[tokio::test]
async fn old_events_go() {
    let hub = Hub::start().await;
    let (alice, bob) = (hub.join("alice").await, hub.join("bob").await);
    let start = 1_700_006_400_000 + 3 * 3_600_000; // 2023-11-15 at 03:00 UTC
    hub.clock.set(start);
    hub.post(
        &alice,
        "/events",
        &json!({"chat": "alice+bob", "kind": "message", "body": {"text": "old"}}),
    )
    .await;
    hub.clock.set(start + 20 * DAY);
    hub.post(
        &alice,
        "/events",
        &json!({"chat": "alice+bob", "kind": "message", "body": {"text": "newer"}}),
    )
    .await;

    hub.clock.set(start + 34 * DAY);
    nightly::run(&hub.state)
        .await
        .expect("the night's job runs");
    // On night 34 events from before day 4 went: the first; the second (day 20) stays.
    let texts: Vec<String> = hub
        .get(&bob, "/chats/alice+bob/events")
        .await
        .body
        .as_array()
        .expect("a list")
        .iter()
        .map(|event| event["body"]["text"].as_str().expect("text").to_owned())
        .collect();
    assert_eq!(texts, ["newer"]);
}
