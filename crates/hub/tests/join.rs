mod common;

use common::{Hub, JOIN_TOKEN};
use serde_json::json;

#[tokio::test]
async fn joining_gives_a_token_and_takes_the_name() {
    let hub = Hub::start().await;
    let token = hub.join("alice").await;
    assert_eq!(token.len(), 64);

    let again = hub
        .post(
            "",
            "/join",
            &json!({"join_token": JOIN_TOKEN, "name": "alice"}),
        )
        .await;
    assert_eq!(again.status, 409);
    assert!(
        again.body["error"]
            .as_str()
            .is_some_and(|error| error.contains("player-two-hub reset"))
    );
}

#[tokio::test]
async fn joining_needs_the_join_token_and_a_good_name() {
    let hub = Hub::start().await;
    let wrong = hub
        .post(
            "",
            "/join",
            &json!({"join_token": "guess", "name": "alice"}),
        )
        .await;
    assert_eq!(wrong.status, 403);
    let bad_name = hub
        .post(
            "",
            "/join",
            &json!({"join_token": JOIN_TOKEN, "name": "Alice!"}),
        )
        .await;
    assert_eq!(bad_name.status, 400);
    assert!(bad_name.body["error"].is_string());
}
