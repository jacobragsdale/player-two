//! Looking for what can run: the agents on Windows and in each WSL distro, and whether each shared
//! folder is there. What was found is kept in `found.json` and the places are advertised from it,
//! so the desk looks in WSL only when it must: when Settings opens, an approval card appears or a
//! run starts. At start it looks on Windows only, which never boots WSL.

use std::path::Path;

use player_two_core::event::RunsIn;

use crate::places::Looked;
use crate::{Desk, agents, places, system};

/// Which systems to look at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Where {
    /// Windows itself.
    Here,
    /// One system: Windows, or one distro.
    At(RunsIn),
    /// Windows and every WSL distro.
    Everywhere,
}

impl Desk {
    /// What the desk found when it last looked, from `found.json` until it looks again.
    pub(crate) fn looked(&self) -> Looked {
        if let Some(looked) = &self.state().looked {
            return looked.clone();
        }
        // A file that cannot be read counts as nothing found; the next look writes it anew.
        let looked: Looked = self
            .inner
            .files
            .read("found.json")
            .ok()
            .flatten()
            .unwrap_or_default();
        self.state().looked = Some(looked.clone());
        looked
    }

    /// Looks again at `systems`, keeps what it found, and advertises the places that can run.
    pub(crate) async fn look(&self, systems: Where) -> anyhow::Result<Looked> {
        // Two looks at once would each write `found.json`, and one would lose.
        let _turn = self.inner.looking.lock().await;
        let settings = self.settings()?;
        let mut looked = self.looked();
        if matches!(
            systems,
            Where::Here | Where::Everywhere | Where::At(RunsIn::Windows)
        ) {
            let found = agents::here(settings.custom_agent.as_deref()).await;
            looked
                .agents
                .retain(|agent| agent.runs_in != RunsIn::Windows);
            looked.agents.extend(found);
            looked
                .missing
                .retain(|(runs_in, _)| runs_in != &RunsIn::Windows);
            for folder in &settings.folders {
                if folder.runs_in == RunsIn::Windows && !Path::new(&folder.path).is_dir() {
                    looked.missing.push((RunsIn::Windows, folder.path.clone()));
                }
            }
        }
        let distros = match &systems {
            Where::Here | Where::At(RunsIn::Windows) => Vec::new(),
            Where::At(RunsIn::Wsl(distro)) => vec![distro.clone()],
            Where::Everywhere => {
                let distros = system::distros().await;
                // What was found in a distro that has gone goes with it.
                let kept = |runs_in: &RunsIn| match runs_in {
                    RunsIn::Windows => true,
                    RunsIn::Wsl(distro) => distros.contains(distro),
                };
                looked.agents.retain(|agent| kept(&agent.runs_in));
                looked.missing.retain(|(runs_in, _)| kept(runs_in));
                distros
            }
        };
        for distro in distros {
            let runs_in = RunsIn::Wsl(distro.clone());
            let found = agents::in_distro(&distro).await;
            looked.agents.retain(|agent| agent.runs_in != runs_in);
            looked.agents.extend(found);
            looked.missing.retain(|(missing, _)| missing != &runs_in);
            for folder in &settings.folders {
                if folder.runs_in == runs_in && !places::exists(&folder.path, &runs_in).await {
                    looked.missing.push((runs_in.clone(), folder.path.clone()));
                }
            }
        }
        self.inner.files.write("found.json", &looked)?;
        self.state().looked = Some(looked.clone());
        self.advertise().await?;
        Ok(looked)
    }

    /// Tells the hub the places that can run, when that changed since it was last told.
    pub(crate) async fn advertise(&self) -> anyhow::Result<()> {
        let Ok(hub) = self.hub() else {
            return Ok(());
        };
        let places = self.settings()?.advertised(&self.looked());
        if self.state().advertised.as_ref() == Some(&places) {
            return Ok(());
        }
        hub.set_places(&places).await?;
        self.state().advertised = Some(places);
        Ok(())
    }
}
