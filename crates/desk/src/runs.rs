//! Running an agent for a request: the run's record in the chat, its session, what its mode lets
//! it do, the steps it waits on, and its reports. Each place runs one thing at a time.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

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

/// How long activity is gathered before it is posted, so that a busy agent posts a few events
/// rather than one for every change.
const BATCH: Duration = Duration::from_millis(400);
/// How long a stopped agent has to stop by itself before it is killed.
const GRACE: Duration = Duration::from_secs(3);

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

/// The runs on this machine.
#[derive(Debug, Default)]
pub(crate) struct Runs {
    pub(crate) active: HashMap<PlaceName, Active>,
    waiting: HashMap<(EventId, String), Waiting>,
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

    /// Whether a place has a run going.
    pub(crate) fn busy(&self, place: &PlaceName) -> Option<EventId> {
        self.runs().active.get(place).map(|active| active.run)
    }

    /// Starts a run for a request, unless its place is busy. A run that cannot start ends at once,
    /// saying why, so that the chat sees it.
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
        let opened = self.open_session(&order).await;
        let (session, folder, said) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                self.end_run(
                    &chat,
                    thread,
                    run,
                    Ending::Failed,
                    Some(format!("{error:#}")),
                )
                .await;
                return Ok(());
            }
        };
        let session = Arc::new(session);
        self.runs().active.insert(
            order.place.clone(),
            Active {
                run,
                session: Arc::clone(&session),
            },
        );
        self.tell(crate::ui::Event::Overview);
        let me = self.me().unwrap_or_else(|| order.asker.clone());
        let text = format!(
            "{}{}",
            preamble(&me, &order.asker, &order.runs_in, &folder, order.mode),
            order.text
        );
        let desk = self.clone();
        tokio::spawn(async move {
            desk.drive(order, chat, thread, run, session, said, text)
                .await
        });
        Ok(())
    }

    /// Finds the folder and the agent for a run, and opens a session with it there.
    async fn open_session(
        &self,
        order: &Order,
    ) -> anyhow::Result<(Session, String, mpsc::UnboundedReceiver<Said>)> {
        let mut found = self.state().agents.clone();
        if found.is_empty() {
            found = self.find_agents().await?;
        }
        let agent = agents::for_system(&found, &order.runs_in).ok_or_else(|| {
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
                .folders
                .iter()
                .find(|folder| folder.name == order.place)
                .ok_or_else(|| anyhow::anyhow!("{} is not shared any more", order.place))?;
            shared.path.clone()
        };
        let (said, heard) = mpsc::unbounded_channel();
        let session = Session::open(agent, &folder, said).await?;
        Ok((session, folder, heard))
    }

    /// Runs one turn, reporting as it goes, and ends the run.
    #[allow(
        clippy::too_many_arguments,
        reason = "one run's parts, gathered where it starts"
    )]
    async fn drive(
        self,
        order: Order,
        chat: ChatId,
        thread: EventId,
        run: EventId,
        session: Arc<Session>,
        mut said: mpsc::UnboundedReceiver<Said>,
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
                    Said::Asks { what, reach, answer } => self.asked(&order, &chat, thread, run, what, reach, answer).await,
                },
                _ = tick.tick() => self.report(&chat, thread, run, &mut batch).await,
            }
        };
        while let Ok(heard) = said.try_recv() {
            if let Said::Activity(item) = heard {
                batch.retain(|kept| kept.id != item.id);
                batch.push(item);
            }
        }
        self.report(&chat, thread, run, &mut batch).await;
        let (how, reason) = match ended {
            Ok(Ok(Ended::Answered(answer))) => {
                let text = if answer.trim().is_empty() {
                    "(The agent said nothing.)".to_owned()
                } else {
                    answer
                };
                if let Err(error) = self
                    .post(
                        &chat,
                        Some(thread),
                        Body::RunResult(RunResult { run, text }),
                    )
                    .await
                {
                    eprintln!("error: cannot post a run's answer: {error:#}");
                }
                (Ending::Done, None)
            }
            Ok(Ok(Ended::Cancelled)) => (Ending::Stopped, None),
            Ok(Err(_)) if self.was_stopped(run) => (Ending::Stopped, None),
            Ok(Err(error)) => (Ending::Failed, Some(format!("{error:#}"))),
            Err(error) => (Ending::Failed, Some(error.to_string())),
        };
        let _ = session.stop().await;
        self.forget_run(run);
        self.end_run(&chat, thread, run, how, reason).await;
    }

    /// Decides a step by the run's mode, or asks a person and leaves the answer to come.
    #[allow(
        clippy::too_many_arguments,
        reason = "a step and the run it belongs to"
    )]
    async fn asked(
        &self,
        order: &Order,
        chat: &ChatId,
        thread: EventId,
        run: EventId,
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
                    .insert((run, prompt.clone()), Waiting { answer, by });
                let body = Body::RunPrompt(RunPrompt {
                    run,
                    prompt: prompt.clone(),
                    what,
                });
                if let Err(error) = self.post(chat, Some(thread), body).await {
                    eprintln!("error: cannot post a step to answer: {error:#}");
                    if let Some(waiting) = self.runs().waiting.remove(&(run, prompt)) {
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
    async fn report(
        &self,
        chat: &ChatId,
        thread: EventId,
        run: EventId,
        batch: &mut Vec<Activity>,
    ) {
        if batch.is_empty() {
            return;
        }
        let items = std::mem::take(batch);
        if let Err(error) = self
            .post(
                chat,
                Some(thread),
                Body::RunActivity(RunActivity { run, items }),
            )
            .await
        {
            eprintln!("error: cannot post a run's activity: {error:#}");
        }
    }

    async fn end_run(
        &self,
        chat: &ChatId,
        thread: EventId,
        run: EventId,
        how: Ending,
        reason: Option<String>,
    ) {
        if let Err(error) = self
            .post(
                chat,
                Some(thread),
                Body::RunEnded(RunEnded { run, how, reason }),
            )
            .await
        {
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
            .contains(&run)
    }

    fn next_prompt(&self) -> u64 {
        self.inner
            .prompts
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Stops a run of this machine's: asks the agent to stop, and kills it if it has not soon.
    pub(crate) fn stop_run(&self, run: EventId) -> anyhow::Result<()> {
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
            .insert(run);
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
