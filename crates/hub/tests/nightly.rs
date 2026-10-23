mod common;

use common::Hub;
use player_two_hub::nightly;
use serde_json::json;

const DAY: i64 = 86_400_000;

#[tokio::test]
async fn old_events_go_and_seven_copies_stay() {
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

    for night in 25..35 {
        hub.clock.set(start + night * DAY);
        nightly::run(&hub.state)
            .await
            .expect("the night's job runs");
    }
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

    let backups = hub.state.config.backups.clone();
    let mut copies: Vec<String> = std::fs::read_dir(&backups)
        .expect("the folder exists")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    copies.sort();
    assert_eq!(copies.len(), 7);
    assert_eq!(
        copies.first().map(String::as_str),
        Some("player-two-2023-12-13.db")
    );
    assert_eq!(
        copies.last().map(String::as_str),
        Some("player-two-2023-12-19.db")
    );

    // A copy is a whole database that opens on its own.
    let copy =
        rusqlite::Connection::open(backups.join("player-two-2023-12-19.db")).expect("it opens");
    let count: i64 = copy
        .query_row("SELECT count(*) FROM events", [], |row| row.get(0))
        .expect("it reads");
    assert_eq!(count, 1);
}
