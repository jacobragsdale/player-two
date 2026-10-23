mod common;

use common::{Hub, overview, until};
use player_two_core::event::RunsIn;
use player_two_desk::ui::{Request, Response};

#[tokio::test]
async fn a_shared_checkout_is_named_by_its_remote_and_advertised() {
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

    let missing = alice
        .handle(Request::AddFolder {
            path: format!("{path}-not-there"),
            runs_in: RunsIn::Windows,
        })
        .await;
    assert!(missing.is_err_and(|error| error.contains("there is no folder")));
    assert_eq!(
        alice
            .handle(Request::AddFolder {
                path: path.clone(),
                runs_in: RunsIn::Windows
            })
            .await,
        Ok(Response::Done)
    );
    let again = alice
        .handle(Request::AddFolder {
            path: path.clone(),
            runs_in: RunsIn::Windows,
        })
        .await;
    assert!(again.is_err_and(|error| error.contains("shared already")));

    let Ok(Response::Places(places)) = alice.handle(Request::Places).await else {
        panic!("the places are listed");
    };
    assert_eq!(places.folders.len(), 1);
    assert_eq!(places.folders[0].name.as_str(), "uploader");
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
