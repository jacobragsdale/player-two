mod common;

use common::Hub;
use player_two_core::names::PersonName;
use player_two_desk::ui::{Request, Response};

#[tokio::test]
async fn a_desk_joins_with_a_code_and_remembers_it() {
    let hub = Hub::start().await;
    let desk = hub.desk("alice");
    let Ok(Response::Setup { joined: None, .. }) = desk.handle(Request::Setup).await else {
        panic!("a new desk has not joined");
    };

    let refused = desk
        .handle(Request::Join {
            code: "hello".to_owned(),
            name: "alice".to_owned(),
        })
        .await;
    assert!(refused.is_err_and(|error| error.contains("not a Player Two join code")));
    let refused = desk
        .handle(Request::Join {
            code: hub.code(),
            name: "Alice Smith".to_owned(),
        })
        .await;
    assert!(refused.is_err_and(|error| error.contains("lowercase letter")));

    let joined = desk
        .handle(Request::Join {
            code: format!("  {}\n", hub.code()),
            name: " alice ".to_owned(),
        })
        .await;
    assert_eq!(joined, Ok(Response::Done));
    let Ok(Response::Setup { joined, .. }) = desk.handle(Request::Setup).await else {
        panic!("setup answers");
    };
    assert_eq!(joined, PersonName::new("alice").ok());

    // The same name from another machine is taken, and the hub says what to do.
    let other = hub.desk("alice-laptop");
    let taken = other
        .handle(Request::Join {
            code: hub.code(),
            name: "alice".to_owned(),
        })
        .await;
    assert!(taken.is_err_and(|error| error.contains("player-two-hub reset")));
}

#[tokio::test]
async fn an_unreachable_hub_is_named() {
    let hub = Hub::start().await;
    let desk = hub.desk("alice");
    let code = player_two_core::join_code::JoinCode {
        hub: "http://127.0.0.1:9".to_owned(),
        token: "t".to_owned(),
    }
    .encode();
    let refused = desk
        .handle(Request::Join {
            code,
            name: "alice".to_owned(),
        })
        .await;
    assert!(
        refused.is_err_and(|error| error.contains("cannot reach the hub at http://127.0.0.1:9"))
    );
}
