//! Finding the agents installed: on Windows itself, and in each WSL distro. An agent is run with
//! the `PATH` found here, so that one installed after the app started is found, and so that an
//! npm-installed agent finds Node.

use std::path::{Path, PathBuf};

use player_two_core::event::RunsIn;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::system;

/// The agents Player Two knows how to start over ACP, in the order it prefers them.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    /// Cursor's command line, `agent acp`.
    Cursor,
    /// GitHub Copilot's command line, `copilot --acp --stdio`.
    Copilot,
    /// Claude Code, through its ACP adapter.
    Claude,
}

impl AgentKind {
    /// Every kind, most preferred first.
    pub const ALL: [Self; 3] = [Self::Cursor, Self::Copilot, Self::Claude];

    /// The program to look for.
    #[must_use]
    pub fn program(self) -> &'static str {
        match self {
            Self::Cursor => "agent",
            Self::Copilot => "copilot",
            Self::Claude => "claude-agent-acp",
        }
    }

    /// What it is told so that it speaks ACP on its standard input and output.
    #[must_use]
    pub fn args(self) -> &'static [&'static str] {
        match self {
            Self::Cursor => &["acp"],
            Self::Copilot => &["--acp", "--stdio"],
            Self::Claude => &[],
        }
    }

    /// Its name for people.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Cursor => "Cursor",
            Self::Copilot => "Copilot",
            Self::Claude => "Claude Code",
        }
    }
}

/// An agent found, ready to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Which agent.
    pub kind: AgentKind,
    /// Where it runs.
    pub runs_in: RunsIn,
    /// The program, by its full path.
    pub program: String,
    /// The arguments that make it speak ACP.
    pub args: Vec<String>,
    /// The `PATH` to run it with.
    pub path: String,
}

/// The separator between a `PATH`'s folders on this system.
const SEPARATOR: char = if cfg!(windows) { ';' } else { ':' };

/// The `PATH` as Windows has it now, machine and user together, not as it was when the app
/// started. Elsewhere, the app's own.
async fn current_path() -> String {
    let own = std::env::var("PATH").unwrap_or_default();
    if !cfg!(windows) {
        return own;
    }
    let mut ask = system::command("powershell.exe");
    ask.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "[Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User')",
    ]);
    match system::output(ask).await {
        Some(fresh) if !fresh.trim().is_empty() => format!("{};{own}", fresh.trim()),
        _ => own,
    }
}

/// The file a program name stands for in a `PATH`: on Windows with each of `PATHEXT`'s endings.
fn search(path: &str, program: &str) -> Option<PathBuf> {
    let endings: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned())
            .split(';')
            .filter(|ending| !ending.is_empty())
            .map(str::to_lowercase)
            .collect()
    } else {
        vec![String::new()]
    };
    path.split(SEPARATOR)
        .filter(|folder| !folder.is_empty())
        .flat_map(|folder| {
            endings
                .iter()
                .map(move |ending| Path::new(folder).join(format!("{program}{ending}")))
        })
        .find(|candidate| candidate.is_file())
}

/// The agents on this system itself.
pub async fn on_this_system() -> Vec<Found> {
    let path = current_path().await;
    AgentKind::ALL
        .into_iter()
        .filter_map(|kind| {
            search(&path, kind.program()).map(|program| Found {
                kind,
                runs_in: RunsIn::Windows,
                program: program.to_string_lossy().into_owned(),
                args: kind.args().iter().map(|arg| (*arg).to_owned()).collect(),
                path: path.clone(),
            })
        })
        .collect()
}

/// Marks the start of what the WSL script prints, past anything a profile prints first.
const MARK: &str = "--player-two--";

/// The agents in a WSL distro. The `PATH` is the one an interactive login shell has, since that
/// is where version managers such as nvm put Node.
pub async fn in_distro(distro: &str) -> Vec<Found> {
    let probes: String = AgentKind::ALL
        .iter()
        .map(|kind| {
            format!(
                "printf '%s\\n' \"$(command -v {} 2>/dev/null)\"; ",
                kind.program()
            )
        })
        .collect();
    let script = format!("printf '{MARK}\\n%s\\n' \"$PATH\"; {probes}");
    let mut command = system::command("wsl.exe");
    command.args(["--distribution", distro, "--exec", "bash", "-lic", &script]);
    let Some(said) = system::output(command).await else {
        return Vec::new();
    };
    parse_distro(distro, &said)
}

/// Reads what the WSL script printed: the mark, the `PATH`, then one line per agent kind, empty
/// when it is not there.
fn parse_distro(distro: &str, said: &str) -> Vec<Found> {
    let Some((_, after)) = said.split_once(&format!("{MARK}\n")) else {
        return Vec::new();
    };
    let mut lines = after.lines();
    let path = lines.next().unwrap_or_default().to_owned();
    AgentKind::ALL
        .into_iter()
        .zip(lines)
        .filter(|(_, line)| line.starts_with('/'))
        .map(|(kind, line)| Found {
            kind,
            runs_in: RunsIn::Wsl(distro.to_owned()),
            program: line.to_owned(),
            args: kind.args().iter().map(|arg| (*arg).to_owned()).collect(),
            path: path.clone(),
        })
        .collect()
}

/// Every agent installed: on this system, then in each WSL distro. An agent of the person's own
/// choosing, a program and its arguments, stands in for any found on this system.
pub async fn everywhere(custom: Option<&[String]>) -> Vec<Found> {
    let mut found = match custom {
        Some([program, args @ ..]) => vec![Found {
            kind: AgentKind::Cursor,
            runs_in: RunsIn::Windows,
            program: program.clone(),
            args: args.to_vec(),
            path: current_path().await,
        }],
        _ => on_this_system().await,
    };
    for distro in system::distros().await {
        found.extend(in_distro(&distro).await);
    }
    found
}

/// The agent to run at a system: the most preferred one found there.
#[must_use]
pub fn for_system<'a>(found: &'a [Found], runs_in: &RunsIn) -> Option<&'a Found> {
    found
        .iter()
        .filter(|agent| &agent.runs_in == runs_in)
        .min_by_key(|agent| agent.kind)
}

#[cfg(test)]
mod tests {
    use player_two_core::event::RunsIn;

    use super::{AgentKind, for_system, parse_distro, search};

    #[test]
    fn a_distros_agents_are_read_past_whatever_the_profile_printed() {
        let said = "Welcome to Ubuntu!\n--player-two--\n/home/bob/.local/bin:/usr/bin\n/home/bob/.local/bin/agent\n\n/home/bob/.nvm/versions/node/v24/bin/claude-agent-acp\n";
        let found = parse_distro("Ubuntu", said);
        let kinds: Vec<AgentKind> = found.iter().map(|agent| agent.kind).collect();
        assert_eq!(kinds, [AgentKind::Cursor, AgentKind::Claude]);
        assert_eq!(found[0].path, "/home/bob/.local/bin:/usr/bin");
        assert_eq!(found[0].args, ["acp"]);
        assert_eq!(
            for_system(&found, &RunsIn::Wsl("Ubuntu".to_owned())).map(|agent| agent.kind),
            Some(AgentKind::Cursor)
        );
        assert!(for_system(&found, &RunsIn::Windows).is_none());
        assert!(parse_distro("Ubuntu", "no mark here").is_empty());
    }

    #[test]
    fn a_program_is_found_on_a_path() {
        let folder = tempfile::tempdir().expect("a folder");
        let name = if cfg!(windows) { "agent.cmd" } else { "agent" };
        std::fs::write(folder.path().join(name), "").expect("written");
        let path = folder.path().to_string_lossy().into_owned();
        assert_eq!(search(&path, "agent"), Some(folder.path().join(name)));
        assert_eq!(search(&path, "copilot"), None);
    }
}
