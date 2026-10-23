//! Phase 1's scenario: two people chat in a channel and directly, one reconnects and catches up,
//! one is reset and joins again, and a third person reads neither chat.

mod common;

use common::{ADMIN_TOKEN, Hub, Lines};
use serde_json::{Value, json};

fn message(chat: &str, text: &str) -> Value {
    json!({"chat": chat, "kind": "message", "body": {"text": text, "tags": []}})
}

/// The next message on a stream, passing over presence.
async fn next_text(lines: &mut Lines) -> (i64, String) {
    loop {
        let line = lines.next().await;
        if line.kind == "message" {
            return (
                line.data["id"].as_i64().expect("an id"),
                line.data["body"]["text"].as_str().expect("text").to_owned(),
            );
        }
    }
}

#[tokio::test]
async fn two_people_chat_and_a_third_reads_nothing() {
    let hub = Hub::start().await;
    let alice = hub.join("alice").await;
    let bob = hub.join("bob").await;
    let carol = hub.join("carol").await;

    // A channel for two.
    hub.post(&alice, "/channels", &json!({"name": "ops"})).await;
    hub.post(&bob, "/channels/ops/join", &json!({})).await;

    let mut bobs = hub.stream(&bob, None).await;
    hub.post(&alice, "/events", &message("#ops", "morning, ops"))
        .await;
    let (first, text) = next_text(&mut bobs).await;
    assert_eq!(text, "morning, ops");

    // Bob goes away; meanwhile Alice writes in the channel and to him directly.
    drop(bobs);
    hub.post(&alice, "/events", &message("#ops", "deploy at noon"))
        .await;
    hub.post(
        &alice,
        "/events",
        &message("alice+bob", "can you look at my proxy?"),
    )
    .await;

    // He comes back and catches up from where he was.
    let mut bobs = hub.stream(&bob, Some(first)).await;
    assert_eq!(next_text(&mut bobs).await.1, "deploy at noon");
    assert_eq!(next_text(&mut bobs).await.1, "can you look at my proxy?");
    hub.post(&bob, "/events", &message("alice+bob", "on it"))
        .await;
    assert_eq!(next_text(&mut bobs).await.1, "on it");

    // Carol is in neither chat.
    assert_eq!(hub.get(&carol, "/chats/%23ops/events").await.status, 403);
    assert_eq!(hub.get(&carol, "/chats/alice+bob/events").await.status, 403);
    let mut carols = hub.stream(&carol, Some(0)).await;
    hub.post(&carol, "/events", &message("bob+carol", "only mine"))
        .await;
    assert_eq!(next_text(&mut carols).await.1, "only mine");

    // Bob gets a new laptop: an admin resets him, and he joins again with his history.
    hub.post(ADMIN_TOKEN, "/admin/reset", &json!({"name": "bob"}))
        .await;
    assert!(bobs.ended().await);
    let bob = hub.join("bob").await;
    let history = hub.get(&bob, "/chats/alice+bob/events").await.body;
    let texts: Vec<&str> = history
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|event| event["body"]["text"].as_str())
        .collect();
    assert_eq!(texts, ["can you look at my proxy?", "on it"]);
}
