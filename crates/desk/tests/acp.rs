//! The desk's ACP client against the scripted fake agent.

use player_two_core::event::RunsIn;
use player_two_desk::acp::{Ended, Said, Session};
use player_two_desk::activity::Reach;
use player_two_desk::agents::{AgentKind, Found};
use tokio::sync::mpsc;

fn fake() -> Found {
    Found {
        kind: AgentKind::Cursor,
        runs_in: RunsIn::Windows,
        program: env!("CARGO_BIN_EXE_fake-acp-agent").to_owned(),
        args: Vec::new(),
        path: std::env::var("PATH").unwrap_or_default(),
        home: std::env::temp_dir().to_string_lossy().into_owned(),
    }
}

/// What a listener heard: the activity, and each ask with its reach.
type Heard = (Vec<player_two_core::event::Activity>, Vec<(String, Reach)>);

/// Answers every ask with `allow`, and keeps the activity and the asks.
fn answering(
    mut said: mpsc::UnboundedReceiver<Said>,
    allow: bool,
) -> tokio::task::JoinHandle<Heard> {
    tokio::spawn(async move {
        let (mut items, mut asks) = (Vec::new(), Vec::new());
        while let Some(said) = said.recv().await {
            match said {
                Said::Activity(item) => items.push(item),
                Said::Asks {
                    what,
                    reach,
                    answer,
                } => {
                    asks.push((what, reach));
                    let _ = answer.send(allow);
                }
            }
        }
        (items, asks)
    })
}

#[tokio::test]
async fn a_turn_reports_what_the_agent_did_and_asks_before_it_changes_anything() {
    let folder = tempfile::tempdir().expect("a folder");
    let (said, heard) = mpsc::unbounded_channel();
    let session = Session::open(&fake(), &folder.path().to_string_lossy(), said)
        .await
        .expect("the session opens");
    let kept = answering(heard, true);
    let ended = session
        .turn("read the proxy settings, then edit and run".to_owned())
        .await
        .expect("the turn ends");
    assert_eq!(
        ended,
        Ended::Answered("Did: read pip.conf, edit done, run done.".to_owned())
    );

    // A second turn in the same session.
    let ended = session
        .turn("nothing".to_owned())
        .await
        .expect("the second turn ends");
    assert_eq!(ended, Ended::Answered("Nothing to do.".to_owned()));
    session.stop().await.expect("it stops");
    drop(session);

    let (items, asks) = kept.await.expect("the listener ends");
    let read = items
        .iter()
        .find(|item| item.id == "read-1")
        .expect("the read is reported");
    assert_eq!(
        (
            read.verb.as_str(),
            read.object.as_str(),
            read.outcome.as_deref()
        ),
        ("read", "pip.conf", Some("done"))
    );
    assert!(
        read.output
            .as_deref()
            .is_some_and(|output| output.contains("old-proxy"))
    );
    let ran = items
        .iter()
        .rev()
        .find(|item| item.id == "run-1")
        .expect("the command is reported");
    assert_eq!(
        (ran.verb.as_str(), ran.outcome.as_deref()),
        ("ran", Some("done"))
    );
    let asked: Vec<&str> = asks.iter().map(|(what, _)| what.as_str()).collect();
    assert_eq!(
        asked,
        [
            "edited pip.conf",
            "ran pip config set global.proxy http://proxy:8080"
        ]
    );
    assert!(asks.iter().all(|(_, reach)| *reach == Reach::Changes));
}

#[tokio::test]
async fn a_refused_step_is_not_done() {
    let folder = tempfile::tempdir().expect("a folder");
    let (said, heard) = mpsc::unbounded_channel();
    let session = Session::open(&fake(), &folder.path().to_string_lossy(), said)
        .await
        .expect("the session opens");
    let _kept = answering(heard, false);
    let ended = session
        .turn("run it".to_owned())
        .await
        .expect("the turn ends");
    assert_eq!(ended, Ended::Answered("Did: run refused.".to_owned()));
}

#[tokio::test]
async fn a_turn_can_be_cancelled() {
    let folder = tempfile::tempdir().expect("a folder");
    let (said, heard) = mpsc::unbounded_channel();
    let session = std::sync::Arc::new(
        Session::open(&fake(), &folder.path().to_string_lossy(), said)
            .await
            .expect("the session opens"),
    );
    let _kept = answering(heard, true);
    let turning = std::sync::Arc::clone(&session);
    let turn = tokio::spawn(async move { turning.turn("be slow".to_owned()).await });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    session.cancel();
    let ended = tokio::time::timeout(std::time::Duration::from_secs(10), turn)
        .await
        .expect("it ends in time")
        .expect("joined");
    assert_eq!(ended.expect("no error"), Ended::Cancelled);
}

#[tokio::test]
async fn an_agent_that_must_sign_in_or_fails_says_so() {
    let folder = tempfile::tempdir().expect("a folder");
    let (said, heard) = mpsc::unbounded_channel();
    let session = Session::open(&fake(), &folder.path().to_string_lossy(), said)
        .await
        .expect("the session opens");
    let _kept = answering(heard, true);
    let signed_out = session.turn("please sign in".to_owned()).await;
    assert!(signed_out.is_err_and(|error| {
        error
            .to_string()
            .contains("Cursor needs signing in: run `agent login`")
    }));
    let failed = session.turn("fail now".to_owned()).await;
    assert!(failed.is_err_and(|error| error.to_string().contains("something broke")));
}

#[tokio::test]
async fn an_agent_that_is_not_there_is_named() {
    let mut missing = fake();
    missing.program = "no-such-agent-anywhere".to_owned();
    let (said, _heard) = mpsc::unbounded_channel();
    let opened = Session::open(&missing, &std::env::temp_dir().to_string_lossy(), said).await;
    assert!(opened.is_err_and(|error| {
        error
            .to_string()
            .contains("cannot start no-such-agent-anywhere")
    }));
}
