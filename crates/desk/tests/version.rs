mod common;

use axum::Router;
use axum::routing::get;
use common::{Hub, until};
use player_two_core::names::PersonName;
use player_two_desk::files::{Files, Identity};
use player_two_desk::ui::Connection;

#[tokio::test]
async fn a_hub_on_another_version_stops_the_desk() {
    // A hub that says it is another version, and answers nothing else.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a free port");
    let base = format!("http://{}", listener.local_addr().expect("an address"));
    let old = Router::new().route(
        "/version",
        get(|| async { axum::Json(serde_json::json!({"version": "0.0.1"})) }),
    );
    tokio::spawn(async move { axum::serve(listener, old).await });

    let hub = Hub::start().await;
    let desk = hub.desk("alice");
    let folder = hub.folder("alice");
    Files::new(folder)
        .save_identity(&Identity {
            hub: base,
            name: PersonName::new("alice").expect("a name"),
            token: "t".to_owned(),
        })
        .expect("saved");
    let running = desk.clone();
    tokio::spawn(async move { running.run().await });
    until(&desk, "the desk to see the other version", |overview| {
        overview.connection
            == Connection::WrongVersion {
                hub: "0.0.1".to_owned(),
            }
    })
    .await;
}
