//! Running an agent for a request: the run's record in the chat, its session, what its mode lets
//! it do, the steps it waits on, and its reports. Each place runs one thing at a time. A session
//! that finished is kept for the asker's follow-ups in the same thread, until the thread has been
//! quiet for an hour.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use player_two_core::event::{
    Activity, Body, Ending, Event, EventId, Mode, RunAccepted, RunActivity, RunEnded, RunPrompt,
    RunResult, RunsIn, StepsBy,
};
use player_two_core::names::{ChatId, PersonName, PlaceName};
use tokio::sync::{mpsc, oneshot};

use crate::Desk;
use crate::acp::{Ended, Said, Session};
use crate::activity::Reach;
use crate::agents;
use crate::mask::mask;

/// How long activity is gathered before it is posted, so that a busy agent posts a few events
/// rather than one for every change.
const BATCH: Duration = Duration::from_millis(400);
/// How long a stopped agent has to stop by itself before it is killed.
const GRACE: Duration = Duration::from_secs(3);
/// How long a session is kept for follow-ups after its thread goes quiet.
pub const QUIET: Duration = Duration::from_secs(3600);

/// What a mode makes of a step the agent asks to take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// It goes ahead.
    Allow,
    /// It does not.
    Refuse,
    /// A person decides.
    Ask,
}

/// What `mode` makes of a step that reaches as far as `reach`.
#[must_use]
pub fn decide(mode: Mode, reach: Reach) -> Decision {
    match (mode, reach) {
        (_, Reach::Looks) | (Mode::Full, Reach::Changes) => Decision::Allow,
        (Mode::ReadOnly, Reach::Changes) => Decision::Refuse,
        (Mode::Ask, Reach::Changes) => Decision::Ask,
    }
}

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
    /// How much it may do.
    pub mode: Mode,
    /// Who answers its steps.
    pub steps_by: StepsBy,
    /// Whether the asker's follow-ups run without a new click.
    pub keep_going: bool,
}

/// What every turn starts with: whose machine, who asked, what it may do, and never to elevate.
#[must_use]
pub fn preamble(
    owner: &PersonName,
    asker: &PersonName,
    runs_in: &RunsIn,
    folder: &str,
    mode: Mode,
) -> String {
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
    let may = match mode {
        Mode::ReadOnly => "You may read and search, but not change anything.",
        Mode::Ask => "You may do what the request needs; each change or command waits for a yes.",
        Mode::Full => "You may do what the request needs.",
    };
    format!(
        "You are an agent on {owner}'s computer, on {system}, in {folder}. {whose} {may} \
         Never try to run anything as administrator: no UAC prompts, runas or sudo. If a step needs admin rights, \
         stop and say exactly what {owner} should run themselves. Keep your answer short: it is posted in the chat.\n\n---\n\n"
    )
}

/// A run going on at one of this machine's places.
#[derive(Debug)]
pub(crate) struct Active {
    pub(crate) run: EventId,
    session: Arc<Session>,
}

/// A step waiting for a yes or no, and who may give it.
#[derive(Debug)]
struct Waiting {
    answer: oneshot::Sender<bool>,
    by: Vec<PersonName>,
}

/// A session, with where its reports arrive and the folder it runs in.
type Parts = (Arc<Session>, mpsc::UnboundedReceiver<Said>, String);

/// A session kept after its run, for the asker's follow-ups in the same thread.
#[derive(Debug)]
pub(crate) struct Kept {
    session: Arc<Session>,
    said: mpsc::UnboundedReceiver<Said>,
    folder: String,
    place: PlaceName,
    keep_going: bool,
    last: Instant,
}

/// The runs on this machine.
#[derive(Debug, Default)]
pub(crate) struct Runs {
    pub(crate) active: HashMap<PlaceName, Active>,
    waiting: HashMap<(EventId, String), Waiting>,
    /// Sessions kept for follow-ups, by the thread they serve.
    kept: HashMap<EventId, Kept>,
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

    /// Whether a run is going on here now.
    pub(crate) fn runs_going(&self, run: EventId) -> bool {
        self.runs().active.values().any(|active| active.run == run)
    }

    /// The places with a run going, by the names they go by now.
    pub(crate) fn runs_going_at(&self) -> Vec<PlaceName> {
        self.runs().active.keys().cloned().collect()
    }

    /// Whether a place has a run going.
    pub(crate) fn busy(&self, place: &PlaceName) -> Option<EventId> {
        let place = self.current(place);
        self.runs().active.get(&place).map(|active| active.run)
    }

    /// The name a place goes by now, so that a request to a renamed folder's former name is the
    /// same place as one to its new name.
    fn current(&self, place: &PlaceName) -> PlaceName {
        self.settings()
            .map_or_else(|_| place.clone(), |settings| settings.current(place))
    }

    /// Starts a run for a request, unless its place is busy. A run that cannot start ends at once,
    /// saying why, so that the chat sees it. A follow-up in a thread whose session is kept goes
    /// to that session; otherwise a new one is given the thread so far.
    pub(crate) async fn start_run(&self, order: Order) -> anyhow::Result<()> {
        anyhow::ensure!(self.busy(&order.place).is_none(), "{} is busy", order.place);
        let chat = order.request.chat.clone();
        let thread = order.request.thread.unwrap_or(order.request.id);
        let accepted = Body::RunAccepted(RunAccepted {
            request: order.request.id,
            place: order.place.clone(),
            runs_in: order.runs_in.clone(),
            mode: order.mode,
            steps_by: order.steps_by,
            keep_going: order.keep_going,
        });
        let run = self.post(&chat, Some(thread), accepted).await?;
        let report = Report { chat, thread, run };
        let place = self.current(&order.place);
        let kept = self
            .runs()
            .kept
            .remove(&thread)
            .filter(|kept| kept.place == place);
        let (parts, earlier) = match kept {
            Some(kept) => ((kept.session, kept.said, kept.folder), String::new()),
            None => match self.open_session(&order).await {
                Ok(parts) => {
                    let earlier = self
                        .state()
                        .chats
                        .as_ref()
                        .map(|chats| chats.transcript(&report.chat, thread, order.request.id))
                        .unwrap_or_default();
                    (parts, earlier)
                }
                Err(error) => {
                    self.end_run(&report, Ending::Failed, Some(format!("{error:#}")))
                        .await;
                    return Ok(());
                }
            },
        };
        self.runs().active.insert(
            place.clone(),
            Active {
                run,
                session: Arc::clone(&parts.0),
            },
        );
        self.tell(crate::ui::Event::Overview);
        let me = self.me().unwrap_or_else(|| order.asker.clone());
        let text = format!(
            "{}{earlier}{}",
            preamble(&me, &order.asker, &order.runs_in, &parts.2, order.mode),
            order.text
        );
        let desk = self.clone();
        tokio::spawn(async move { desk.drive(order, report, parts, text).await });
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
    /// session for follow-ups; one that was stopped or failed stops it.
    async fn drive(
        self,
        order: Order,
        report: Report,
        (session, mut said, folder): Parts,
        text: String,
    ) {
        let turning = Arc::clone(&session);
        let mut turn = tokio::spawn(async move { turning.turn(text).await });
        let mut batch: Vec<Activity> = Vec::new();
        let mut tick = tokio::time::interval(BATCH);
        let ended = loop {
            tokio::select! {
                ended = &mut turn => break ended,
                Some(heard) = said.recv() => match heard {
                    Said::Activity(item) => {
                        batch.retain(|kept| kept.id != item.id);
                        batch.push(item);
                    }
                    Said::Asks { what, reach, answer } => self.asked(&order, &report, what, reach, answer).await,
                },
                _ = tick.tick() => self.report(&report, &mut batch).await,
            }
        };
        while let Ok(heard) = said.try_recv() {
            if let Said::Activity(item) = heard {
                batch.retain(|kept| kept.id != item.id);
                batch.push(item);
            }
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
                    text: mask(&text),
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
        self.forget_run(report.run);
        if how == Ending::Done {
            let kept = Kept {
                session,
                said,
                folder,
                place: self.current(&order.place),
                keep_going: order.keep_going,
                last: Instant::now(),
            };
            self.runs().kept.insert(report.thread, kept);
        } else {
            let _ = session.stop().await;
        }
        self.end_run(&report, how, reason).await;
    }

    /// Decides a step by the run's mode, or asks a person and leaves the answer to come.
    async fn asked(
        &self,
        order: &Order,
        report: &Report,
        what: String,
        reach: Reach,
        answer: oneshot::Sender<bool>,
    ) {
        match decide(order.mode, reach) {
            Decision::Allow => {
                let _ = answer.send(true);
            }
            Decision::Refuse => {
                let _ = answer.send(false);
            }
            Decision::Ask => {
                let prompt = format!("p{}", self.next_prompt());
                let owner = self.me().unwrap_or_else(|| order.asker.clone());
                let by = match order.steps_by {
                    StepsBy::Owner => vec![owner],
                    StepsBy::Asker => vec![owner, order.asker.clone()],
                };
                self.runs()
                    .waiting
                    .insert((report.run, prompt.clone()), Waiting { answer, by });
                let body = Body::RunPrompt(RunPrompt {
                    run: report.run,
                    prompt: prompt.clone(),
                    what: mask(&what),
                });
                if let Err(error) = self.post(&report.chat, Some(report.thread), body).await {
                    eprintln!("error: cannot post a step to answer: {error:#}");
                    if let Some(waiting) = self.runs().waiting.remove(&(report.run, prompt)) {
                        let _ = waiting.answer.send(false);
                    }
                }
            }
        }
    }

    /// Passes a yes or no to the step it answers, if its author may give it.
    pub(crate) fn answered(&self, event: &Event) {
        let Body::RunAnswer(answer) = &event.body else {
            return;
        };
        let mut runs = self.runs();
        let key = (answer.run, answer.prompt.clone());
        if runs
            .waiting
            .get(&key)
            .is_some_and(|waiting| waiting.by.contains(&event.author))
            && let Some(waiting) = runs.waiting.remove(&key)
        {
            let _ = waiting.answer.send(answer.allow);
        }
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

    fn forget_run(&self, run: EventId) {
        let mut runs = self.runs();
        runs.active.retain(|_, active| active.run != run);
        runs.waiting
            .retain(|(waiting_run, _), _| *waiting_run != run);
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

    fn next_prompt(&self) -> u64 {
        self.inner
            .prompts
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Whether a follow-up in `thread` at `place` may run without a click: its session is kept,
    /// the owner let the asker keep going, and the thread has not been quiet for an hour.
    pub(crate) fn keeps_going(&self, thread: EventId, place: &PlaceName) -> bool {
        let place = self.current(place);
        self.runs().kept.get(&thread).is_some_and(|kept| {
            kept.place == place && kept.keep_going && kept.last.elapsed() < QUIET
        })
    }

    /// Stops the sessions kept for threads quiet for an hour.
    pub(crate) async fn forget_quiet(&self) {
        let quiet: Vec<Kept> = {
            let mut runs = self.runs();
            let threads: Vec<EventId> = runs
                .kept
                .iter()
                .filter(|(_, kept)| kept.last.elapsed() >= QUIET)
                .map(|(thread, _)| *thread)
                .collect();
            threads
                .iter()
                .filter_map(|thread| runs.kept.remove(thread))
                .collect()
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
                .find(|active| active.run == run)
                .ok_or_else(|| anyhow::anyhow!("that run is not going on here"))?;
            Arc::clone(&active.session)
        };
        self.inner
            .stopped
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(run, by);
        session.cancel();
        tokio::spawn(async move {
            tokio::time::sleep(GRACE).await;
            let _ = session.stop().await;
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use player_two_core::event::{Mode, RunsIn};
    use player_two_core::names::PersonName;

    use super::{Decision, decide, preamble};
    use crate::activity::Reach;

    #[test]
    fn modes_decide_steps() {
        assert_eq!(decide(Mode::ReadOnly, Reach::Looks), Decision::Allow);
        assert_eq!(decide(Mode::ReadOnly, Reach::Changes), Decision::Refuse);
        assert_eq!(decide(Mode::Ask, Reach::Changes), Decision::Ask);
        assert_eq!(decide(Mode::Full, Reach::Changes), Decision::Allow);
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
            Mode::Ask,
        );
        assert!(text.contains("on alice's computer, on the WSL distro Ubuntu, in /home/alice"));
        assert!(text.contains("jacob asked"));
        assert!(text.contains("not jacob's"));
        assert!(text.contains("each change or command waits for a yes"));
        assert!(text.contains("no UAC prompts, runas or sudo"));
        let own = preamble(
            &alice,
            &alice,
            &RunsIn::Windows,
            "C:\\Users\\alice",
            Mode::Full,
        );
        assert!(!own.contains("not alice's"));
    }
}
