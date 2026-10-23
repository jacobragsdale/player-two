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
