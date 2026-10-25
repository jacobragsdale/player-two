//! Running an agent for a request: the run's record in the chat, its session, and its reports. A
//! run its owner let go may do anything: whatever the agent still asks about is allowed. Each
//! place runs one thing at a time. A session that finished is kept for the next tag of its place
//! in the same thread, until an hour after it last finished.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use player_two_core::event::{
    Activity, Body, Ending, Event, EventId, MAX_TEXT, RunAccepted, RunActivity, RunEnded,
    RunResult, RunsIn,
};
use player_two_core::names::{ChatId, PersonName, PlaceName};
use tokio::sync::mpsc;

use crate::Desk;
use crate::acp::{Ended, Session};
use crate::agents;
use crate::mask::mask;

/// How long activity is gathered before it is posted, so that a busy agent posts a few events
/// rather than one for every change.
const BATCH: Duration = Duration::from_millis(400);
/// How long a stopped agent has to stop by itself before it is killed.
const GRACE: Duration = Duration::from_secs(3);
/// How long a session is kept for follow-ups after it last finished.
pub const QUIET: Duration = Duration::from_secs(3600);

/// What a run is for.
#[derive(Debug, Clone)]
pub struct Order {
    /// The message asking.
    pub request: Event,
    /// Who asked.
    pub asker: PersonName,
    /// What they asked, as typed.
    pub text: String,
    /// The place it runs at.
    pub place: PlaceName,
    /// Where on the machine.
    pub runs_in: RunsIn,
    /// Whether the asker's follow-ups run without a new click.
    pub keep_going: bool,
}

/// What follows an answer cut to fit in an event.
const CUT: &str = "\n\n(The rest of the answer was cut: it was longer than the chat holds.)";

/// An answer as the chat may hold it: whole when it fits, or else its start and a note saying
/// it was cut.
#[must_use]
pub fn fitted(answer: &str) -> String {
    if answer.chars().count() <= MAX_TEXT {
        return answer.to_owned();
    }
    let kept: String = answer
        .chars()
        .take(MAX_TEXT - CUT.chars().count())
        .collect();
    format!("{kept}{CUT}")
}

/// What every turn starts with: whose machine, who asked, what it may do, that the chat sees all
/// it reads or prints, and never to elevate.
#[must_use]
pub fn preamble(owner: &PersonName, asker: &PersonName, runs_in: &RunsIn, folder: &str) -> String {
    let system = match runs_in {
        RunsIn::Windows => "Windows".to_owned(),
        RunsIn::Wsl(distro) => format!("the WSL distro {distro}"),
    };
    let whose = if owner == asker {
        format!("{owner} asked for what follows, through Player Two, the chat their team shares.")
    } else {
        format!(
            "{asker} asked for what follows, through Player Two, the chat their team shares. This is {owner}'s computer, not {asker}'s; {owner} let the request run."
        )
    };
    format!(
        "You are an agent on {owner}'s computer, on {system}, in {folder}. {whose} You may do what the request needs. \
         Everything you read or print is shown to everyone in this chat, so never print passwords, tokens or keys. \
         Never try to run anything as administrator: no UAC prompts, runas or sudo. If a step needs admin rights, \
         stop and say exactly what {owner} should run themselves. Keep your answer short: it is posted in the chat.\n\n---\n\n"
    )
}

/// A run going on, or starting, at one of this machine's places. The place is held from the
/// moment a run starts, so that a second cannot start beside it.
#[derive(Debug)]
pub(crate) struct Active {
    /// The run, once its `run.accepted` is posted.
    pub(crate) run: Option<EventId>,
    /// Its session, once open.
    session: Option<Arc<Session>>,
    /// Who asked for it.
    asker: PersonName,
}

/// A session, with where its reports arrive and the folder it runs in.
type Parts = (Arc<Session>, mpsc::UnboundedReceiver<Activity>, String);

/// A session kept after its run, for the next tag of its place in the same thread.
#[derive(Debug)]
pub(crate) struct Kept {
    thread: EventId,
    session: Arc<Session>,
    said: mpsc::UnboundedReceiver<Activity>,
    folder: String,
    place: PlaceName,
    runs_in: RunsIn,
    /// The asker the owner let keep going here, whose follow-ups run without a click.
    keeps: Option<PersonName>,
    last: Instant,
}

/// The runs on this machine.
#[derive(Debug, Default)]
pub(crate) struct Runs {
    /// The runs going on or starting, by the name their place had when they started.
    pub(crate) active: HashMap<PlaceName, Active>,
    /// Sessions kept for follow-ups: one for each thread and place.
    kept: Vec<Kept>,
}

/// Where a run reports: its chat, its thread and its id.
#[derive(Debug, Clone)]
struct Report {
    chat: ChatId,
    thread: EventId,
    run: EventId,
}

impl Desk {
    fn runs(&self) -> std::sync::MutexGuard<'_, Runs> {
        self.inner
            .runs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether a run is going on here now, or starting.
    pub(crate) fn runs_going(&self, run: EventId) -> bool {
        self.runs()
            .active
            .values()
            .any(|active| active.run == Some(run))
    }

    /// The places with a run going or starting, by the names they go by now.
    pub(crate) fn runs_going_at(&self) -> Vec<PlaceName> {
        let settings = self.settings().unwrap_or_default();
        self.runs()
            .active
            .keys()
            .map(|place| settings.current(place))
            .collect()
    }

    /// The runs going on or starting here, in words: where, and for whom. Quitting or updating
    /// the app would stop them.
    #[must_use]
    pub fn running(&self) -> Vec<String> {
        let settings = self.settings().unwrap_or_default();
        let me = self.me();
        let mut running: Vec<String> = self
            .runs()
            .active
            .iter()
            .map(|(place, active)| {
                let place = settings.current(place);
                let place = if place.is_machine() {
                    "your computer".to_owned()
                } else {
                    place.to_string()
                };
                if me.as_ref() == Some(&active.asker) {
                    format!("{place}, for you")
                } else {
                    format!("{place}, for {}", active.asker)
                }
            })
            .collect();
        running.sort();
        running
    }

    /// Whether a place has a run going or starting. Places are compared by the names they go by
    /// now, so that a renamed folder is the same place under its old name and its new one.
    pub(crate) fn busy(&self, place: &PlaceName) -> bool {
        let settings = self.settings().unwrap_or_default();
        let place = settings.current(place);
        self.runs()
            .active
            .keys()
            .any(|held| settings.current(held) == place)
    }

    /// Starts a run for a request, unless its place is busy. A run that cannot start ends at once,
    /// saying why, so that the chat sees it. A tag of a place in a thread where its session is
    /// kept goes to that session, whoever wrote it; otherwise a new one is given what ran there
    /// so far.
    pub(crate) async fn start_run(&self, order: Order) -> anyhow::Result<()> {
        let settings = self.settings()?;
        let place = settings.current(&order.place);
        {
            // Checked and held in one step, so that two starts cannot both find the place free.
            let mut runs = self.runs();
            anyhow::ensure!(
                !runs
                    .active
                    .keys()
                    .any(|held| settings.current(held) == place),
                "{} is busy",
                order.place
            );
            runs.active.insert(
                order.place.clone(),
                Active {
                    run: None,
                    session: None,
                    asker: order.asker.clone(),
                },
            );
        }
        let chat = order.request.chat.clone();
        let thread = order.request.thread.unwrap_or(order.request.id);
        let accepted = Body::RunAccepted(RunAccepted {
            request: order.request.id,
            place: order.place.clone(),
            runs_in: order.runs_in.clone(),
            keep_going: order.keep_going,
        });
        let run = match self.post(&chat, Some(thread), accepted).await {
            Ok(run) => run,
            Err(error) => {
                self.runs().active.remove(&order.place);
                return Err(error);
            }
        };
        if let Some(active) = self.runs().active.get_mut(&order.place) {
            active.run = Some(run);
        }
        self.tell(crate::ui::Event::Overview);
        let report = Report { chat, thread, run };
        let me = self.me().unwrap_or_else(|| order.asker.clone());
        let (kept, elsewhere) = {
            let mut runs = self.runs();
            let at = runs
                .kept
                .iter()
                .position(|kept| kept.thread == thread && settings.current(&kept.place) == place);
            match at.map(|at| runs.kept.remove(at)) {
                Some(kept) if kept.runs_in == order.runs_in => (Some(kept), None),
                other => (None, other),
            }
        };
        // A session kept on another system is no use to a run on this one.
        if let Some(elsewhere) = elsewhere {
            let _ = elsewhere.session.stop().await;
        }
        let keeps = kept.as_ref().and_then(|kept| kept.keeps.clone());
        let (parts, earlier) = match kept {
            Some(kept) => ((kept.session, kept.said, kept.folder), String::new()),
            None => match self.open_session(&order).await {
                Ok(parts) => {
                    let ran_here = |ran: &PlaceName| settings.current(ran) == place;
                    let earlier = self
                        .state()
                        .chats
                        .as_ref()
                        .map(|chats| {
                            chats.transcript(&report.chat, thread, &me, ran_here, order.request.id)
                        })
                        .unwrap_or_default();
                    (parts, earlier)
                }
                Err(error) => {
                    self.forget_run(run);
                    self.end_run(&report, Ending::Failed, Some(format!("{error:#}")))
                        .await;
                    return Ok(());
                }
            },
        };
        // Stop while it started is kept, and found here, under the same lock it is taken with.
        let stopped = {
            let mut runs = self.runs();
            let stopped = self.was_stopped(run);
            if let Some(active) = runs.active.get_mut(&order.place)
                && !stopped
            {
                active.session = Some(Arc::clone(&parts.0));
            }
            stopped
        };
        if stopped {
            let _ = parts.0.stop().await;
            self.forget_run(run);
            self.end_run(&report, Ending::Stopped, self.stopped_by(run))
                .await;
            return Ok(());
        }
        let text = format!(
            "{}{earlier}{}",
            preamble(&me, &order.asker, &order.runs_in, &parts.2),
            order.text
        );
        let desk = self.clone();
        tokio::spawn(async move { desk.drive(order, report, parts, text, keeps).await });
        Ok(())
    }

    /// Finds the folder and the agent for a run, and opens a session with it there.
    async fn open_session(&self, order: &Order) -> anyhow::Result<Parts> {
        let found = self
            .look(crate::looking::Where::At(order.runs_in.clone()))
            .await?
            .agents;
        let chosen = self.settings()?.agents.get(&order.runs_in).copied();
        let agent = agents::for_system(&found, &order.runs_in, chosen).ok_or_else(|| {
            let system = match &order.runs_in {
                RunsIn::Windows => "Windows".to_owned(),
                RunsIn::Wsl(distro) => distro.clone(),
            };
            anyhow::anyhow!(
                "no agent is installed on {system}; the setup page says how to install one"
            )
        })?;
        let folder = if order.place.is_machine() {
            agent.home.clone()
        } else {
            let settings = self.settings()?;
            let shared = settings
                .folder(&order.place)
                .ok_or_else(|| anyhow::anyhow!("{} is not shared any more", order.place))?;
            shared.path.clone()
        };
        let (said, heard) = mpsc::unbounded_channel();
        let session = Session::open(agent, &folder, said).await?;
        Ok((Arc::new(session), heard, folder))
    }

    /// Runs one turn, reporting as it goes, and ends the run. A run that finished keeps its
    /// session for follow-ups, with whoever may keep going there: this run's asker when the
    /// owner let them, or else whoever could before. One that was stopped or failed stops it.
    async fn drive(
        self,
        order: Order,
        report: Report,
        (session, mut said, folder): Parts,
        text: String,
        keeps: Option<PersonName>,
    ) {
        let turning = Arc::clone(&session);
        let mut turn = tokio::spawn(async move { turning.turn(text).await });
        let mut batch: Vec<Activity> = Vec::new();
        let mut tick = tokio::time::interval(BATCH);
        let ended = loop {
            tokio::select! {
                ended = &mut turn => break ended,
                Some(item) = said.recv() => {
                    batch.retain(|kept| kept.id != item.id);
                    batch.push(item);
                }
                _ = tick.tick() => self.report(&report, &mut batch).await,
            }
        };
        while let Ok(item) = said.try_recv() {
            batch.retain(|kept| kept.id != item.id);
            batch.push(item);
        }
        self.report(&report, &mut batch).await;
        let (how, reason) = match ended {
            Ok(Ok(Ended::Answered(answer))) => {
                let text = if answer.trim().is_empty() {
                    "(The agent said nothing.)".to_owned()
                } else {
                    answer
                };
                let result = Body::RunResult(RunResult {
                    run: report.run,
                    text: fitted(&mask(&text)),
                });
                if let Err(error) = self.post(&report.chat, Some(report.thread), result).await {
                    eprintln!("error: cannot post a run's answer: {error:#}");
                }
                (Ending::Done, None)
            }
            Ok(Ok(Ended::Cancelled)) => (Ending::Stopped, self.stopped_by(report.run)),
            Ok(Err(_)) if self.was_stopped(report.run) => {
                (Ending::Stopped, self.stopped_by(report.run))
            }
            Ok(Err(error)) => (Ending::Failed, Some(format!("{error:#}"))),
            Err(error) => (Ending::Failed, Some(error.to_string())),
        };
        // A Stop pressed as it finished still ends the session, and keeping going with it.
        let stopped = self.finish_run(report.run);
        if how == Ending::Done && !stopped {
            let kept = Kept {
                thread: report.thread,
                session,
                said,
                folder,
                place: order.place.clone(),
                runs_in: order.runs_in.clone(),
                keeps: if order.keep_going {
                    Some(order.asker.clone())
                } else {
                    keeps
                },
                last: Instant::now(),
            };
            self.runs().kept.push(kept);
        } else {
            let _ = session.stop().await;
        }
        self.end_run(&report, how, reason).await;
    }

    /// Posts what the agent did since the last report.
    async fn report(&self, report: &Report, batch: &mut Vec<Activity>) {
        if batch.is_empty() {
            return;
        }
        let items = std::mem::take(batch)
            .into_iter()
            .map(|item| Activity {
                id: item.id,
                verb: mask(&item.verb),
                object: mask(&item.object),
                outcome: item.outcome.as_deref().map(mask),
                output: item.output.as_deref().map(mask),
            })
            .collect();
        let body = Body::RunActivity(RunActivity {
            run: report.run,
            items,
        });
        if let Err(error) = self.post(&report.chat, Some(report.thread), body).await {
            eprintln!("error: cannot post a run's activity: {error:#}");
        }
    }

    async fn end_run(&self, report: &Report, how: Ending, reason: Option<String>) {
        // A failure can quote what the agent printed.
        let body = Body::RunEnded(RunEnded {
            run: report.run,
            how,
            reason: reason.as_deref().map(mask),
        });
        if let Err(error) = self.post(&report.chat, Some(report.thread), body).await {
            eprintln!("error: cannot post how a run ended: {error:#}");
        }
        self.tell(crate::ui::Event::Overview);
    }

    /// Lets go of a run's place, and says whether it was stopped. Stop looks for the run under
    /// the same lock, so it either finds the run going and is seen here, or finds it gone.
    fn finish_run(&self, run: EventId) -> bool {
        let mut runs = self.runs();
        runs.active.retain(|_, active| active.run != Some(run));
        self.was_stopped(run)
    }

    fn forget_run(&self, run: EventId) {
        self.runs()
            .active
            .retain(|_, active| active.run != Some(run));
    }

    fn was_stopped(&self, run: EventId) -> bool {
        self.inner
            .stopped
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(&run)
    }

    /// Why a stopped run stopped, when its asker stopped it rather than its owner.
    fn stopped_by(&self, run: EventId) -> Option<String> {
        self.inner
            .stopped
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&run)
            .cloned()
            .flatten()
            .map(|asker| format!("{asker} stopped it"))
    }

    /// Stops a run of this machine's when its asker posted `run.stop`, as if the owner had
    /// pressed Stop.
    pub(crate) fn asked_to_stop(&self, event: &Event) {
        let Body::RunStop(stop) = &event.body else {
            return;
        };
        let asker = self
            .state()
            .chats
            .as_ref()
            .and_then(|chats| chats.asker_of(&event.chat, stop.run));
        if asker.as_ref() == Some(&event.author)
            && self.me().as_ref() != Some(&event.author)
            && self.runs_going(stop.run)
            && let Err(error) = self.stop_run(stop.run, Some(event.author.clone()))
        {
            eprintln!("error: cannot stop a run its asker stopped: {error:#}");
        }
    }

    /// Where `asker`'s tag of `place` in `thread` runs without a click, if it may: its session is
    /// kept, the owner let them keep going, and the agent last finished there less than an hour
    /// ago.
    pub(crate) fn keeps_going(
        &self,
        thread: EventId,
        place: &PlaceName,
        asker: &PersonName,
    ) -> Option<RunsIn> {
        let settings = self.settings().unwrap_or_default();
        let place = settings.current(place);
        self.runs()
            .kept
            .iter()
            .find(|kept| {
                kept.thread == thread
                    && settings.current(&kept.place) == place
                    && kept.keeps.as_ref() == Some(asker)
                    && kept.last.elapsed() < QUIET
            })
            .map(|kept| kept.runs_in.clone())
    }

    /// Stops the sessions kept an hour after they last finished.
    pub(crate) async fn forget_quiet(&self) {
        let quiet: Vec<Kept> = {
            let mut runs = self.runs();
            let (quiet, fresh) = std::mem::take(&mut runs.kept)
                .into_iter()
                .partition(|kept| kept.last.elapsed() >= QUIET);
            runs.kept = fresh;
            quiet
        };
        for kept in quiet {
            let _ = kept.session.stop().await;
        }
    }

    /// Stops a run of this machine's: asks the agent to stop, and kills it if it has not soon.
    /// Stopping ends keeping going in its thread too, since the session goes with it. `by` is the
    /// asker, when it was they who stopped it.
    pub(crate) fn stop_run(&self, run: EventId, by: Option<PersonName>) -> anyhow::Result<()> {
        let session = {
            let runs = self.runs();
            let active = runs
                .active
                .values()
                .find(|active| active.run == Some(run))
                .ok_or_else(|| anyhow::anyhow!("that run is not going on here"))?;
            // Whoever stopped it first is who stopped it.
            self.inner
                .stopped
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entry(run)
                .or_insert(by);
            active.session.clone()
        };
        // A run still starting finds itself stopped once its session opens.
        if let Some(session) = session {
            session.cancel();
            tokio::spawn(async move {
                tokio::time::sleep(GRACE).await;
                let _ = session.stop().await;
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use player_two_core::event::{MAX_TEXT, RunsIn};
    use player_two_core::names::PersonName;

    use super::{fitted, preamble};

    #[test]
    fn a_long_answer_is_cut_to_fit_and_says_so() {
        assert_eq!(fitted("Fixed."), "Fixed.");
        let long = "é".repeat(MAX_TEXT + 10);
        let cut = fitted(&long);
        assert_eq!(cut.chars().count(), MAX_TEXT);
        assert!(cut.starts_with("éé") && cut.ends_with("longer than the chat holds.)"));
    }

    #[test]
    fn the_preamble_says_whose_machine_and_never_to_elevate() {
        let alice = PersonName::new("alice").expect("a name");
        let jacob = PersonName::new("jacob").expect("a name");
        let text = preamble(
            &alice,
            &jacob,
            &RunsIn::Wsl("Ubuntu".to_owned()),
            "/home/alice",
        );
        assert!(text.contains("on alice's computer, on the WSL distro Ubuntu, in /home/alice"));
        assert!(text.contains("jacob asked"));
        assert!(text.contains("not jacob's"));
        assert!(text.contains("You may do what the request needs."));
        assert!(
            text.contains(
                "shown to everyone in this chat, so never print passwords, tokens or keys"
            )
        );
        assert!(text.contains("no UAC prompts, runas or sudo"));
        let own = preamble(&alice, &alice, &RunsIn::Windows, "C:\\Users\\alice");
        assert!(!own.contains("not alice's"));
    }
}
