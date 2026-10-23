mod common;

use common::Hub;
use serde_json::json;

#[tokio::test]
async fn channels_are_made_joined_and_left() {
    let hub = Hub::start().await;
    let alice = hub.join("alice").await;
    let bob = hub.join("bob").await;

    assert_eq!(
        hub.post(&alice, "/channels", &json!({"name": "ops"}))
            .await
            .status,
        201
    );
    assert_eq!(
        hub.post(&bob, "/channels", &json!({"name": "ops"}))
            .await
            .status,
        409
    );
    assert_eq!(
        hub.post(&bob, "/channels/ops/join", &json!({}))
            .await
            .status,
        204
    );
    assert_eq!(
        hub.post(&bob, "/channels/nope/join", &json!({}))
            .await
            .status,
        404
    );
    assert_eq!(
        hub.post(&bob, "/channels/Bad!/join", &json!({}))
            .await
            .status,
        400
    );
    assert_eq!(
        hub.get(&bob, "/channels").await.body,
        json!([{"name": "ops", "members": ["alice", "bob"]}])
    );

    assert_eq!(
        hub.post(&alice, "/channels/ops/leave", &json!({}))
            .await
            .status,
        204
    );
    assert_eq!(
        hub.get(&bob, "/channels").await.body,
        json!([{"name": "ops", "members": ["bob"]}])
    );
}

#[tokio::test]
async fn every_route_but_join_needs_a_personal_token() {
    let hub = Hub::start().await;
    let refused = hub.get("not-a-token", "/channels").await;
    assert_eq!(refused.status, 401);
    assert!(
        refused.body["error"]
            .as_str()
            .is_some_and(|error| error.contains("join code"))
    );
}
