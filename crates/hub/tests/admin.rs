mod common;

use common::{ADMIN_TOKEN, Hub};
use player_two_core::join_code::JoinCode;
use serde_json::json;

#[tokio::test]
async fn the_join_code_holds_the_hubs_address_and_join_token() {
    let hub = Hub::start().await;
    assert_eq!(
        hub.post("guess", "/admin/join-code", &json!({}))
            .await
            .status,
        401
    );
    let reply = hub.post(ADMIN_TOKEN, "/admin/join-code", &json!({})).await;
    let code = JoinCode::decode(reply.body["code"].as_str().expect("a code")).expect("it decodes");
    assert_eq!(
        code,
        JoinCode {
            hub: "https://hub.example".to_owned(),
            token: common::JOIN_TOKEN.to_owned()
        }
    );
}

#[tokio::test]
async fn a_reset_lets_someone_join_again_and_ends_their_old_stream() {
    let hub = Hub::start().await;
    let (old, bob) = (hub.join("alice").await, hub.join("bob").await);
    hub.post(
        &old,
        "/events",
        &json!({"chat": "alice+bob", "kind": "message", "body": {"text": "from the old laptop"}}),
    )
    .await;
    let mut stream = hub.stream(&old, None).await;
    stream.next().await;

    assert_eq!(
        hub.post(&bob, "/admin/reset", &json!({"name": "alice"}))
            .await
            .status,
        401
    );
    assert_eq!(
        hub.post(ADMIN_TOKEN, "/admin/reset", &json!({"name": "nobody"}))
            .await
            .status,
        404
    );
    assert_eq!(
        hub.post(ADMIN_TOKEN, "/admin/reset", &json!({"name": "alice"}))
            .await
            .status,
        204
    );
    assert!(stream.ended().await, "the old stream ends");
    assert_eq!(hub.get(&old, "/channels").await.status, 401);

    let new = hub.join("alice").await;
    let history = hub.get(&new, "/chats/alice+bob/events").await.body;
    assert_eq!(history[0]["body"]["text"], "from the old laptop");
}

#[tokio::test]
async fn removing_someone_takes_them_off_the_hub_and_keeps_what_they_wrote() {
    let hub = Hub::start().await;
    let (alice, bob) = (hub.join("alice").await, hub.join("bob").await);
    hub.post(&alice, "/channels", &json!({"name": "ops"})).await;
    hub.post(&bob, "/channels/ops/join", &json!({})).await;
    hub.put(
        &bob,
        "/places",
        &json!([{"name": "machine", "kind": "machine"}]),
    )
    .await;
    for chat in ["#ops", "alice+bob"] {
        hub.post(
            &bob,
            "/events",
            &json!({"chat": chat, "kind": "message", "body": {"text": "bye"}}),
        )
        .await;
    }
    let mut stream = hub.stream(&bob, None).await;
    stream.next().await;

    assert_eq!(
        hub.post(&alice, "/admin/remove", &json!({"name": "bob"}))
            .await
            .status,
        401
    );
    assert_eq!(
        hub.post(ADMIN_TOKEN, "/admin/remove", &json!({"name": "nobody"}))
            .await
            .status,
        404
    );
    assert_eq!(
        hub.post(ADMIN_TOKEN, "/admin/remove", &json!({"name": "bob"}))
            .await
            .status,
        204
    );
    assert!(stream.ended().await, "their stream ends");
    assert_eq!(hub.get(&bob, "/channels").await.status, 401);
    let people = hub.get(&alice, "/people").await.body;
    let names: Vec<&str> = people
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|person| person["name"].as_str())
        .collect();
    assert_eq!(names, ["alice"]);
    let channels = hub.get(&alice, "/channels").await.body;
    assert_eq!(channels[0]["members"], json!(["alice"]));
    for chat in ["%23ops", "alice+bob"] {
        let kept = hub.get(&alice, &format!("/chats/{chat}/events")).await.body;
        assert_eq!(
            kept[0]["body"]["text"], "bye",
            "what bob wrote in {chat} stays"
        );
    }

    // Joining again under the name brings them back.
    hub.join("bob").await;
    let people = hub.get(&alice, "/people").await.body;
    assert_eq!(people.as_array().map(Vec::len), Some(2));
}
