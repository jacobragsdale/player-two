mod common;

use common::Hub;
use serde_json::{Value, json};

fn message(chat: &str, text: &str) -> Value {
    json!({"chat": chat, "kind": "message", "body": {"text": text, "tags": []}})
}

#[tokio::test]
async fn events_and_presence_reach_who_may_see_them() {
    let hub = Hub::start().await;
    let (alice, bob, carol) = (
        hub.join("alice").await,
        hub.join("bob").await,
        hub.join("carol").await,
    );
    let mut alices = hub.stream(&alice, None).await;
    assert_eq!(
        alices.next().await.data,
        json!({"name": "alice", "here": true})
    );

    let mut carols = hub.stream(&carol, None).await;
    let seen = alices.next().await;
    assert_eq!(
        (seen.kind.as_str(), seen.data),
        ("presence", json!({"name": "carol", "here": true}))
    );
    carols.next().await; // Her own arrival.

    hub.post(&bob, "/events", &message("alice+bob", "hello alice"))
        .await;
    let seen = alices.next().await;
    assert_eq!(seen.kind, "message");
    assert_eq!(seen.data["body"]["text"], "hello alice");
    assert_eq!(seen.data["author"], "bob");

    // Carol hears nothing of a chat that is not hers; the next thing she hears is her own.
    hub.post(
        &carol,
        "/events",
        &message("alice+carol", "hello from carol"),
    )
    .await;
    assert_eq!(carols.next().await.data["body"]["text"], "hello from carol");

    // Alice is in that chat, so she hears it.
    assert_eq!(alices.next().await.data["body"]["text"], "hello from carol");

    drop(carols);
    let seen = alices.next().await;
    assert_eq!(
        (seen.kind.as_str(), seen.data),
        ("presence", json!({"name": "carol", "here": false}))
    );
}

#[tokio::test]
async fn a_stream_resumes_after_the_last_event_it_had() {
    let hub = Hub::start().await;
    let (alice, bob) = (hub.join("alice").await, hub.join("bob").await);
    let mut ids = Vec::new();
    for text in ["one", "two", "three"] {
        ids.push(
            hub.post(&alice, "/events", &message("alice+bob", text))
                .await
                .body["id"]
                .as_i64()
                .expect("an id"),
        );
    }
    let mut bobs = hub.stream(&bob, Some(ids[0])).await;
    let mut texts = Vec::new();
    while texts.len() < 2 {
        let line = bobs.next().await;
        if line.kind == "message" {
            texts.push(line.data["body"]["text"].as_str().expect("text").to_owned());
        }
    }
    assert_eq!(texts, ["two", "three"]);

    hub.post(&alice, "/events", &message("alice+bob", "four"))
        .await;
    let mut next = bobs.next().await;
    while next.kind != "message" {
        next = bobs.next().await;
    }
    assert_eq!(next.data["body"]["text"], "four");
}
