mod common;

use std::sync::{Arc, Mutex};

use common::{Hub, overview, until};
use player_two_core::event::RunsIn;
use player_two_desk::Desk;
use player_two_desk::ui::{Request, Response};

/// Gives the desk a folder dialog that picks whatever `picked` holds when it opens.
fn dialog(desk: &Desk) -> Arc<Mutex<Option<String>>> {
    let picked = Arc::new(Mutex::new(None));
    let held = Arc::clone(&picked);
    desk.set_picker(Arc::new(move || {
        let folder = held.lock().expect("not poisoned").clone();
        Box::pin(async move { folder })
    }));
    picked
}

#[tokio::test]
async fn a_picked_checkout_is_named_by_its_remote_and_advertised() {
    let hub = Hub::start().await;
    let alice = hub.joined("alice").await;
    let bob = hub.joined("bob").await;

    let checkout = tempfile::tempdir().expect("a folder");
    let path = checkout.path().join("whatever-the-folder-is-called");
    std::fs::create_dir(&path).expect("made");
    for args in [
        vec!["init", "-q"],
        vec![
            "remote",
            "add",
            "origin",
            "https://dev.azure.com/acme/Payments/_git/Uploader",
        ],
    ] {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(args)
            .status()
            .expect("git runs");
        assert!(status.success());
    }
    let path = path.to_string_lossy().into_owned();
    let picked = dialog(&alice);

    // Cancelling the dialog shares nothing.
    assert_eq!(alice.handle(Request::PickFolder).await, Ok(Response::Done));
    *picked.lock().expect("not poisoned") = Some(format!("{path}-not-there"));
    let missing = alice.handle(Request::PickFolder).await;
    assert!(missing.is_err_and(|error| error.contains("there is no folder")));
    *picked.lock().expect("not poisoned") = Some(path.clone());
    assert_eq!(alice.handle(Request::PickFolder).await, Ok(Response::Done));
    let again = alice.handle(Request::PickFolder).await;
    assert!(again.is_err_and(|error| error.contains("shared already")));

    let Ok(Response::Places(places)) = alice.handle(Request::Places).await else {
        panic!("the places are listed");
    };
    assert_eq!(places.folders.len(), 1);
    assert_eq!(places.folders[0].name.as_str(), "uploader");
    assert_eq!(places.folders[0].runs_in, RunsIn::Windows);
    assert_eq!(places.machine_runs_in, RunsIn::Windows);

    // Changing her places tells everyone to fetch the people again.
    let seen = until(&bob, "alice's places", |overview| {
        overview
            .people
            .iter()
            .any(|person| person.name.as_str() == "alice" && person.places.len() == 2)
    })
    .await;
    let names: Vec<&str> = seen
        .people
        .iter()
        .find(|person| person.name.as_str() == "alice")
        .expect("alice")
        .places
        .iter()
        .map(|place| place.name.as_str())
        .collect();
    assert_eq!(names, ["machine", "uploader"]);

    // Renamed, it is advertised by its new name, and its old one still reaches it.
    let uploader = places.folders[0].name.clone();
    let refused = alice
        .handle(Request::RenameFolder {
            name: uploader.clone(),
            to: "machine".to_owned(),
        })
        .await;
    assert!(refused.is_err(), "machine is not a folder's name");
    assert_eq!(
        alice
            .handle(Request::RenameFolder {
                name: uploader.clone(),
                to: "up".to_owned(),
            })
            .await,
        Ok(Response::Done)
    );
    until(&bob, "alice's renamed folder", |overview| {
        overview.people.iter().any(|person| {
            person.name.as_str() == "alice"
                && person
                    .places
                    .iter()
                    .any(|place| place.name.as_str() == "up")
        })
    })
    .await;
    let Ok(Response::Approval { choices, .. }) = alice
        .handle(Request::Approval {
            place: uploader.clone(),
        })
        .await
    else {
        panic!("the old name still finds the folder");
    };
    assert_eq!(choices, [RunsIn::Windows]);

    let Ok(Response::Places(places)) = alice.handle(Request::Places).await else {
        panic!("the places are listed");
    };
    assert_eq!(places.folders[0].former, [uploader]);
    let name = places.folders[0].name.clone();
    assert_eq!(
        alice.handle(Request::RemoveFolder { name }).await,
        Ok(Response::Done)
    );
    let Ok(Response::Places(places)) = alice.handle(Request::Places).await else {
        panic!("the places are listed");
    };
    assert!(places.folders.is_empty());
    let _ = overview(&alice).await;
}

#[tokio::test]
async fn a_desk_without_a_dialog_says_so() {
    let hub = Hub::start().await;
    let alice = hub.joined("alice").await;
    let picked = alice.handle(Request::PickFolder).await;
    assert!(picked.is_err_and(|error| error.contains("folder dialog")));
}
