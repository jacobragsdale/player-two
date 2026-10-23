mod common;

use common::Hub;
use serde_json::{Value, json};

fn message(chat: &str, text: &str, tags: Value) -> Value {
    json!({"chat": chat, "kind": "message", "body": {"text": text, "tags": tags}})
}

fn accepted(chat: &str, request: i64, steps_by: &str) -> Value {
    json!({"chat": chat, "kind": "run.accepted", "body": {
        "request": request, "place": "machine", "runs_in": "windows", "mode": "ask", "steps_by": steps_by, "keep_going": false
    }})
}

fn answer(chat: &str, run: i64) -> Value {
    json!({"chat": chat, "kind": "run.answer", "body": {"run": run, "prompt": "p1", "allow": true}})
}

#[tokio::test]
async fn a_request_and_its_run_follow_the_rules() {
    let hub = Hub::start().await;
    let (alice, bob, carol) = (
        hub.join("alice").await,
        hub.join("bob").await,
        hub.join("carol").await,
    );
    let chat = "alice+bob";

    let request = hub
        .post(
            &alice,
            "/events",
            &message(
                chat,
                "@bob fix pip",
                json!([{"person": "bob", "place": "machine"}]),
            ),
        )
        .await;
    let request = request.body["id"].as_i64().expect("an id");

    // Only the two people in a direct chat may use it.
    assert_eq!(
        hub.post(&carol, "/events", &message(chat, "hi", json!([])))
            .await
            .status,
        403
    );
    assert_eq!(hub.get(&carol, "/chats/alice+bob/events").await.status, 403);

    // Only the tagged place's owner may accept.
    assert_eq!(
        hub.post(&alice, "/events", &accepted(chat, request, "owner"))
            .await
            .status,
        403
    );
    let run = hub
        .post(&bob, "/events", &accepted(chat, request, "owner"))
        .await;
    let run = run.body["id"].as_i64().expect("an id");

    // The run's reports are the owner's; the asker answers steps only when handed them.
    let activity = json!({"chat": chat, "kind": "run.activity", "body": {"run": run, "items": []}});
    assert_eq!(hub.post(&bob, "/events", &activity).await.status, 200);
    assert_eq!(hub.post(&alice, "/events", &activity).await.status, 403);
    assert_eq!(
        hub.post(&alice, "/events", &answer(chat, run)).await.status,
        403
    );
    assert_eq!(
        hub.post(&bob, "/events", &answer(chat, run)).await.status,
        200
    );

    let handed = hub
        .post(&bob, "/events", &accepted(chat, request, "asker"))
        .await;
    let handed = handed.body["id"].as_i64().expect("an id");
    assert_eq!(
        hub.post(&alice, "/events", &answer(chat, handed))
            .await
            .status,
        200
    );

    // Only the asker may take a request back.
    let withdrawn = json!({"chat": chat, "kind": "run.withdrawn", "body": {"request": request}});
    assert_eq!(hub.post(&bob, "/events", &withdrawn).await.status, 403);
    assert_eq!(hub.post(&alice, "/events", &withdrawn).await.status, 200);

    // A run is reported in the chat it started in, and nowhere else.
    hub.post(&alice, "/channels", &json!({"name": "ops"})).await;
    hub.post(&bob, "/channels/ops/join", &json!({})).await;
    let elsewhere =
        json!({"chat": "#ops", "kind": "run.result", "body": {"run": run, "text": "done"}});
    assert_eq!(hub.post(&bob, "/events", &elsewhere).await.status, 403);
}

#[tokio::test]
async fn threads_start_with_a_message() {
    let hub = Hub::start().await;
    let (alice, _bob) = (hub.join("alice").await, hub.join("bob").await);
    let first = hub
        .post(&alice, "/events", &message("alice+bob", "first", json!([])))
        .await;
    let first = first.body["id"].as_i64().expect("an id");
    let mut reply = message("alice+bob", "a reply", json!([]));
    reply["thread"] = json!(first);
    let reply = hub.post(&alice, "/events", &reply).await;
    assert_eq!(reply.status, 200);

    let mut nested = message("alice+bob", "a reply to a reply", json!([]));
    nested["thread"] = reply.body["id"].clone();
    assert_eq!(hub.post(&alice, "/events", &nested).await.status, 403);
}

#[tokio::test]
async fn a_chat_is_read_a_page_at_a_time() {
    let hub = Hub::start().await;
    let (alice, bob) = (hub.join("alice").await, hub.join("bob").await);
    for n in 1..=5 {
        hub.post(
            &alice,
            "/events",
            &message("alice+bob", &format!("m{n}"), json!([])),
        )
        .await;
    }
    let texts = |page: &Value| -> Vec<String> {
        page.as_array()
            .expect("a list")
            .iter()
            .map(|event| event["body"]["text"].as_str().expect("text").to_owned())
            .collect()
    };
    let newest = hub.get(&bob, "/chats/alice+bob/events?limit=2").await.body;
    assert_eq!(texts(&newest), ["m4", "m5"]);
    let before = newest[0]["id"].as_i64().expect("an id");
    let older = hub
        .get(
            &bob,
            &format!("/chats/alice+bob/events?before={before}&limit=10"),
        )
        .await
        .body;
    assert_eq!(texts(&older), ["m1", "m2", "m3"]);
    assert_eq!(newest[0]["author"], "alice");
    assert_eq!(newest[0]["kind"], "message");
    assert_eq!(newest[0]["ts"], 1_700_000_000_000_i64);
}

#[tokio::test]
async fn text_has_a_limit() {
    let hub = Hub::start().await;
    let (alice, _bob) = (hub.join("alice").await, hub.join("bob").await);
    let long = "x".repeat(64_001);
    let refused = hub
        .post(&alice, "/events", &message("alice+bob", &long, json!([])))
        .await;
    assert_eq!(refused.status, 403);
}
