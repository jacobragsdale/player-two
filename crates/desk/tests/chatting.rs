mod common;

use common::{Hub, chat, overview, until, until_texts};
use player_two_core::names::ChatId;
use player_two_desk::ui::{Connection, Request, Response};

fn id(chat: &str) -> ChatId {
    ChatId::try_from(chat.to_owned()).expect("a chat id")
}

fn send(chat: &str, text: &str) -> Request {
    Request::Send {
        chat: id(chat),
        thread: None,
        text: text.to_owned(),
        mode: None,
    }
}

#[tokio::test]
async fn two_desks_chat_directly_and_in_a_channel() {
    let hub = Hub::start().await;
    let alice = hub.joined("alice").await;
    let bob = hub.joined("bob").await;

    // Each sees the other once the lists are fetched again, which joining a channel does.
    assert_eq!(
        alice
            .handle(Request::CreateChannel {
                name: "#ops".to_owned()
            })
            .await,
        Ok(Response::Done)
    );
    assert_eq!(
        bob.handle(Request::JoinChannel {
            name: "ops".to_owned()
        })
        .await,
        Ok(Response::Done)
    );
    assert_eq!(
        alice.handle(send("#ops", "morning")).await,
        Ok(Response::Done)
    );
    until_texts(&bob, "#ops", None, &["morning"]).await;

    assert_eq!(
        bob.handle(send("alice+bob", "can you look at my proxy?"))
            .await,
        Ok(Response::Done)
    );
    let summary = until(&alice, "bob's message to count as unread", |overview| {
        overview
            .chats
            .iter()
            .any(|summary| summary.title == "bob" && summary.unread == 1)
    })
    .await;
    assert_eq!(summary.me.map(String::from).as_deref(), Some("alice"));
    let view = chat(&alice, "alice+bob", None).await;
    assert_eq!(view.first_unread, view.items.first().map(|item| item.id));

    // A reply goes in a thread under it.
    let question = view.items[0].id;
    let reply = Request::Send {
        chat: id("alice+bob"),
        thread: Some(question),
        text: "on it".to_owned(),
        mode: None,
    };
    assert_eq!(alice.handle(reply).await, Ok(Response::Done));
    until_texts(
        &bob,
        "alice+bob",
        Some(question),
        &["can you look at my proxy?", "on it"],
    )
    .await;
    assert_eq!(chat(&bob, "alice+bob", None).await.items[0].replies, 1);

    assert_eq!(
        alice
            .handle(Request::Read {
                chat: id("alice+bob")
            })
            .await,
        Ok(Response::Done)
    );
    let read = overview(&alice).await;
    assert!(read.chats.iter().all(|summary| summary.unread == 0));
    assert!(alice.handle(send("alice+bob", "   ")).await.is_err());
}

#[tokio::test]
async fn a_reset_sends_the_desk_back_to_joining() {
    let hub = Hub::start().await;
    let bob = hub.joined("bob").await;
    hub.reset("bob").await;
    until(&bob, "the desk to see it must join again", |overview| {
        overview.connection == Connection::NotJoined
    })
    .await;
    let Ok(Response::Setup { joined: None, .. }) = bob.handle(Request::Setup).await else {
        panic!("the first-run page shows again");
    };
    // Joining again under the same name works, and the desk reconnects by itself.
    assert_eq!(
        bob.handle(Request::Join {
            code: hub.code(),
            name: "bob".to_owned()
        })
        .await,
        Ok(Response::Done)
    );
    until(&bob, "it reconnects", |overview| {
        overview.connection == Connection::Connected
    })
    .await;
}
