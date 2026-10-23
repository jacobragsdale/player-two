mod common;

use common::Hub;
use serde_json::json;

#[tokio::test]
async fn people_are_listed_with_their_places_and_presence() {
    let hub = Hub::start().await;
    let (alice, bob) = (hub.join("alice").await, hub.join("bob").await);
    let places =
        json!([{"name": "machine", "kind": "machine"}, {"name": "uploader", "kind": "repo"}]);
    assert_eq!(hub.put(&alice, "/places", &places).await.status, 204);

    let mut alices = hub.stream(&alice, None).await;
    alices.next().await;
    let people = hub.get(&bob, "/people").await.body;
    assert_eq!(
        people,
        json!([
            {"name": "alice", "here": true, "places": [{"name": "machine", "kind": "machine"}, {"name": "uploader", "kind": "repo"}]},
            {"name": "bob", "here": false, "places": []}
        ])
    );

    // Leaving is recorded by the hub's clock.
    hub.clock.set(1_700_000_123_000);
    drop(alices);
    let mut seen = None;
    for _ in 0..50 {
        let people = hub.get(&bob, "/people").await.body;
        if people[0]["here"] == json!(false) && people[0]["last_seen"].is_number() {
            seen = people[0]["last_seen"].as_i64();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(seen, Some(1_700_000_123_000));

    // A new list replaces the old one.
    hub.put(
        &alice,
        "/places",
        &json!([{"name": "machine", "kind": "machine"}]),
    )
    .await;
    assert_eq!(
        hub.get(&bob, "/people").await.body[0]["places"],
        json!([{"name": "machine", "kind": "machine"}])
    );
}

#[tokio::test]
async fn only_the_machine_is_called_machine() {
    let hub = Hub::start().await;
    let alice = hub.join("alice").await;
    for bad in [
        json!([{"name": "machine", "kind": "repo"}]),
        json!([{"name": "uploader", "kind": "machine"}]),
    ] {
        assert_eq!(hub.put(&alice, "/places", &bad).await.status, 400);
    }
}
