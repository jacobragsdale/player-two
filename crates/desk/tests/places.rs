mod common;

use std::sync::{Arc, Mutex};

use common::{Hub, overview, until};
use player_two_core::event::RunsIn;
use player_two_desk::Desk;
use player_two_desk::agents::AgentKind;
use player_two_desk::files::Files;
use player_two_desk::places::Settings;
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
    common::fake_agent(&hub, &alice, "alice").await;

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

    let kept: Settings = Files::new(hub.folder("alice"))
        .read("settings.json")
        .expect("read")
        .expect("there");
    assert_eq!(kept.folders[0].former, [uploader]);

    // A folder that has gone is kept, marked, and no longer advertised.
    std::fs::rename(&path, format!("{path}-moved")).expect("moved");
    let Ok(Response::Places(places)) = alice.handle(Request::Places).await else {
        panic!("the places are listed");
    };
    assert_eq!(
        places.folders[0].unavailable.as_deref(),
        Some("the folder is not there any more")
    );
    until(&bob, "the gone folder to go", |overview| {
        overview.people.iter().any(|person| {
            person.name.as_str() == "alice"
                && person.places.len() == 1
                && person.places[0].name.is_machine()
        })
    })
    .await;
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

#[tokio::test]
async fn the_agent_chosen_for_a_system_is_kept() {
    let hub = Hub::start().await;
    let alice = hub.joined("alice").await;
    common::fake_agent(&hub, &alice, "alice").await;
    let chose = alice
        .handle(Request::ChooseAgent {
            runs_in: RunsIn::Windows,
            kind: AgentKind::Claude,
        })
        .await;
    assert_eq!(chose, Ok(Response::Done));
    let kept: Settings = Files::new(hub.folder("alice"))
        .read("settings.json")
        .expect("read")
        .expect("there");
    assert_eq!(kept.agents.get(&RunsIn::Windows), Some(&AgentKind::Claude));
    // Claude Code is not there, so the agent that is still runs.
    let Ok(Response::Places(places)) = alice.handle(Request::Places).await else {
        panic!("the places are listed");
    };
    assert_eq!(places.agents.len(), 1);
    assert!(places.agents[0].chosen);
}

// On Windows this would run the real installer, and the tests need no network.
#[cfg(not(windows))]
#[tokio::test]
async fn cursor_is_installed_from_here_on_windows_only() {
    let hub = Hub::start().await;
    let alice = hub.joined("alice").await;
    let installed = alice.handle(Request::InstallCursor).await;
    assert!(installed.is_err_and(|error| error.contains("Windows only")));
    let Ok(Response::Places(places)) = alice.handle(Request::Places).await else {
        panic!("the places are listed");
    };
    assert!(!places.wsl, "there is no WSL here");
}

#[tokio::test]
async fn an_agent_that_needs_signing_in_is_signed_in_over_acp() {
    let hub = Hub::start().await;
    let alice = hub.joined("alice").await;
    let marker = hub.folder("alice").join("signed-in");
    let agent = |args: &[&str]| Settings {
        custom_agent: Some(
            std::iter::once(env!("CARGO_BIN_EXE_fake-acp-agent"))
                .chain(args.iter().copied())
                .map(str::to_owned)
                .collect(),
        ),
        ..Settings::default()
    };
    let files = Files::new(hub.folder("alice"));
    let marker_path = marker.to_string_lossy().into_owned();
    files
        .write("settings.json", &agent(&["--needs-sign-in", &marker_path]))
        .expect("written");

    let checked = alice
        .handle(Request::CheckAgent {
            runs_in: RunsIn::Windows,
        })
        .await;
    let Ok(Response::Checked {
        ready: false,
        sign_in: true,
        message,
    }) = checked
    else {
        panic!("the check says to sign in, not {checked:?}");
    };
    assert!(message.contains("needs signing in"));
    let signed = alice
        .handle(Request::SignIn {
            runs_in: RunsIn::Windows,
        })
        .await;
    assert!(
        matches!(signed, Ok(Response::Checked { ready: true, .. })),
        "signing in, then checking, finds it ready: {signed:?}"
    );
    assert!(marker.exists(), "the agent was asked to authenticate");

    // An agent with no way to sign in over ACP is left to its own command.
    files
        .write("settings.json", &agent(&["--no-sign-in"]))
        .expect("written");
    let signed = alice
        .handle(Request::SignIn {
            runs_in: RunsIn::Windows,
        })
        .await;
    let Ok(Response::Checked {
        ready: false,
        sign_in: false,
        message,
    }) = signed
    else {
        panic!("the agent cannot be signed in from here, not {signed:?}");
    };
    assert!(message.contains("run `agent login`"), "{message}");
}
