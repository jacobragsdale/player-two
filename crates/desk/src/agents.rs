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
    /// Cursor's command line, `agent --force acp`.
    Cursor,
    /// GitHub Copilot's command line, `copilot --acp --stdio --allow-all`.
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

    /// What it is told so that it speaks ACP on its standard input and output, and does what
    /// it needs without asking. Claude Code's adapter is put in its mode for that once its
    /// session opens.
    #[must_use]
    pub fn args(self) -> &'static [&'static str] {
        match self {
            // A global option, so it goes before the command.
            Self::Cursor => &["--force", "acp"],
            Self::Copilot => &["--acp", "--stdio", "--allow-all"],
            Self::Claude => &[],
        }
    }

    /// Whether a program found by this kind's name is this kind's. Another tool may be called
    /// `agent` too, so only one that lives in Cursor's own install folder is Cursor's: on Windows
    /// `%LOCALAPPDATA%\cursor-agent`, and elsewhere `~/.local/share/cursor-agent`, which
    /// `~/.local/bin/agent` links into.
    #[must_use]
    pub fn owns(self, program: &Path) -> bool {
        match self {
            Self::Cursor => std::fs::canonicalize(program)
                .unwrap_or_else(|_| program.to_owned())
                .components()
                .any(|part| part.as_os_str().eq_ignore_ascii_case(CURSOR_FOLDER)),
            Self::Copilot | Self::Claude => true,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// The home folder there, where a request for the machine runs.
    pub home: String,
}

/// The folder Cursor's installer puts its agent in.
const CURSOR_FOLDER: &str = "cursor-agent";

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
    search_for(path, program, |_| true)
}

/// The first file a program name stands for in a `PATH` that `wanted` accepts.
fn search_for(path: &str, program: &str, wanted: impl Fn(&Path) -> bool) -> Option<PathBuf> {
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
        .find(|candidate| candidate.is_file() && wanted(candidate))
}

/// The person's home folder on this system.
fn home() -> String {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default()
}

/// The agents on this system itself.
pub async fn on_this_system() -> Vec<Found> {
    let path = current_path().await;
    AgentKind::ALL
        .into_iter()
        .filter_map(|kind| {
            search_for(&path, kind.program(), |program| kind.owns(program)).map(|program| Found {
                kind,
                runs_in: RunsIn::Windows,
                program: program.to_string_lossy().into_owned(),
                args: kind.args().iter().map(|arg| (*arg).to_owned()).collect(),
                path: path.clone(),
                home: home(),
            })
        })
        .collect()
}

/// Marks the start of what the WSL script prints, past anything a profile prints first.
const MARK: &str = "--player-two--";

/// The agents in a WSL distro, or `None` when the distro could not be asked. The `PATH` is the
/// one an interactive login shell has, since that is where version managers such as nvm put Node.
pub async fn in_distro(distro: &str) -> Option<Vec<Found>> {
    let probes: String = AgentKind::ALL
        .iter()
        .map(|kind| match kind {
            // The first `agent` on the `PATH` that links into Cursor's install folder.
            AgentKind::Cursor => format!(
                "printf '%s\\n' \"$(IFS=:; for d in $PATH; do f=\"$d/{}\"; [ -f \"$f\" ] && [ -x \"$f\" ] || continue; case \"$(readlink -f \"$f\")\" in */{CURSOR_FOLDER}/*) printf '%s' \"$f\"; break;; esac; done)\"; ",
                kind.program()
            ),
            AgentKind::Copilot | AgentKind::Claude => format!(
                "printf '%s\\n' \"$(command -v {} 2>/dev/null)\"; ",
                kind.program()
            ),
        })
        .collect();
    let script = format!("printf '{MARK}\\n%s\\n%s\\n' \"$PATH\" \"$HOME\"; {probes}");
    let mut command = system::command("wsl.exe");
    command.args(["--distribution", distro, "--exec", "bash", "-lic", &script]);
    parse_distro(distro, &system::output(command).await?)
}

/// Reads what the WSL script printed: the mark, the `PATH`, the home folder, then one line per
/// agent kind, empty when it is not there. Without the mark, the script did not run.
fn parse_distro(distro: &str, said: &str) -> Option<Vec<Found>> {
    let (_, after) = said.split_once(&format!("{MARK}\n"))?;
    let mut lines = after.lines();
    let path = lines.next().unwrap_or_default().to_owned();
    let home = lines.next().unwrap_or_default().to_owned();
    let found = AgentKind::ALL
        .into_iter()
        .zip(lines)
        .filter(|(_, line)| line.starts_with('/'))
        .map(|(kind, line)| Found {
            kind,
            runs_in: RunsIn::Wsl(distro.to_owned()),
            program: line.to_owned(),
            args: kind.args().iter().map(|arg| (*arg).to_owned()).collect(),
            path: path.clone(),
            home: home.clone(),
        })
        .collect();
    Some(found)
}

/// The agents on this system itself. An agent of the person's own choosing, a program and its
/// arguments, stands in for any found.
pub async fn here(custom: Option<&[String]>) -> Vec<Found> {
    match custom {
        Some([program, args @ ..]) => vec![Found {
            kind: AgentKind::Cursor,
            runs_in: RunsIn::Windows,
            program: program.clone(),
            args: args.to_vec(),
            path: current_path().await,
            home: home(),
        }],
        _ => on_this_system().await,
    }
}

/// The official install of Cursor's agent on Windows. The desk runs it, and only it, when the
/// person presses Set up; the window never sends a command.
pub const CURSOR_INSTALL: &str = "irm 'https://cursor.com/install?win32=true' | iex";

/// How long the installer may take before it counts as stuck.
const INSTALLING: std::time::Duration = std::time::Duration::from_secs(300);

/// The most of what the installer printed that is kept to show.
const INSTALL_OUTPUT: usize = 4000;

/// Runs Cursor's official installer in PowerShell, as the person, and gives back whether it
/// succeeded and the end of what it printed.
pub async fn install_cursor() -> anyhow::Result<(bool, String)> {
    anyhow::ensure!(
        cfg!(windows),
        "Cursor's agent is installed from here on Windows only"
    );
    let mut command = system::command("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", CURSOR_INSTALL]);
    let output = tokio::time::timeout(INSTALLING, command.output())
        .await
        .map_err(|_| anyhow::anyhow!("the installer was still going after five minutes"))??;
    let printed = format!(
        "{}{}",
        system::text(&output.stdout),
        system::text(&output.stderr)
    );
    let printed = printed.trim();
    let start = printed
        .char_indices()
        .rev()
        .nth(INSTALL_OUTPUT)
        .map_or(0, |(at, _)| at);
    Ok((output.status.success(), printed[start..].to_owned()))
}

/// The agents installed only in part on this system, so that the person can be told what is
/// missing: the Cursor editor without its agent, or Claude Code without its ACP adapter.
pub async fn half_installed(found: &[Found]) -> Vec<AgentKind> {
    half_installed_on(&current_path().await, found)
}

/// [`half_installed`], on a given `PATH`.
fn half_installed_on(path: &str, found: &[Found]) -> Vec<AgentKind> {
    [(AgentKind::Cursor, "cursor"), (AgentKind::Claude, "claude")]
        .into_iter()
        .filter(|(kind, part)| {
            search(path, part).is_some()
                && !found
                    .iter()
                    .any(|agent| agent.kind == *kind && agent.runs_in == RunsIn::Windows)
        })
        .map(|(kind, _)| kind)
        .collect()
}

/// The agent to run at a system: the one chosen there when it is found, or else the most
/// preferred one found there.
#[must_use]
pub fn for_system<'a>(
    found: &'a [Found],
    runs_in: &RunsIn,
    chosen: Option<AgentKind>,
) -> Option<&'a Found> {
    let there = || found.iter().filter(|agent| &agent.runs_in == runs_in);
    there()
        .find(|agent| Some(agent.kind) == chosen)
        .or_else(|| there().min_by_key(|agent| agent.kind))
}

#[cfg(test)]
mod tests {
    use player_two_core::event::RunsIn;

    use super::{AgentKind, for_system, half_installed_on, parse_distro, search, search_for};

    #[test]
    fn a_distros_agents_are_read_past_whatever_the_profile_printed() {
        let said = "Welcome to Ubuntu!\n--player-two--\n/home/bob/.local/bin:/usr/bin\n/home/bob\n/home/bob/.local/bin/agent\n\n/home/bob/.nvm/versions/node/v24/bin/claude-agent-acp\n";
        let found = parse_distro("Ubuntu", said).expect("the script ran");
        let kinds: Vec<AgentKind> = found.iter().map(|agent| agent.kind).collect();
        assert_eq!(kinds, [AgentKind::Cursor, AgentKind::Claude]);
        assert_eq!(found[0].path, "/home/bob/.local/bin:/usr/bin");
        assert_eq!(found[0].home, "/home/bob");
        assert_eq!(found[0].args, ["--force", "acp"]);
        let ubuntu = RunsIn::Wsl("Ubuntu".to_owned());
        let chosen = |choice| for_system(&found, &ubuntu, choice).map(|agent| agent.kind);
        assert_eq!(chosen(None), Some(AgentKind::Cursor));
        assert_eq!(chosen(Some(AgentKind::Claude)), Some(AgentKind::Claude));
        assert_eq!(
            chosen(Some(AgentKind::Copilot)),
            Some(AgentKind::Cursor),
            "a choice not there falls back to the preferred"
        );
        assert!(for_system(&found, &RunsIn::Windows, None).is_none());
        assert!(
            parse_distro("Ubuntu", "no mark here").is_none(),
            "a distro that could not be asked is not one with no agents"
        );
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

    #[test]
    fn only_cursors_own_agent_is_cursor() {
        let folder = tempfile::tempdir().expect("a folder");
        let name = if cfg!(windows) { "agent.cmd" } else { "agent" };
        let other = folder.path().join("grok").join("bin");
        let cursors = folder.path().join("cursor-agent");
        for dir in [&other, &cursors] {
            std::fs::create_dir_all(dir).expect("made");
            std::fs::write(dir.join(name), "").expect("written");
        }
        assert!(!AgentKind::Cursor.owns(&other.join(name)));
        assert!(AgentKind::Cursor.owns(&cursors.join(name)));
        assert!(AgentKind::Copilot.owns(&other.join(name)));
        // Another tool's `agent` comes first on the PATH, and is passed over.
        let path = std::env::join_paths([&other, &cursors])
            .expect("a PATH")
            .to_string_lossy()
            .into_owned();
        let found = search_for(&path, "agent", |program| AgentKind::Cursor.owns(program));
        assert_eq!(found, Some(cursors.join(name)));
        #[cfg(unix)]
        {
            // On Linux the installer links ~/.local/bin/agent into its versions folder.
            let bin = folder.path().join("bin");
            std::fs::create_dir(&bin).expect("made");
            std::os::unix::fs::symlink(cursors.join(name), bin.join("agent")).expect("linked");
            assert!(AgentKind::Cursor.owns(&bin.join("agent")));
        }
    }

    #[test]
    fn an_editor_without_its_agent_is_half_installed() {
        let folder = tempfile::tempdir().expect("a folder");
        let ending = if cfg!(windows) { ".cmd" } else { "" };
        for program in ["cursor", "claude", "claude-agent-acp"] {
            std::fs::write(folder.path().join(format!("{program}{ending}")), "").expect("written");
        }
        let path = folder.path().to_string_lossy().into_owned();
        let claude = super::Found {
            kind: AgentKind::Claude,
            runs_in: RunsIn::Windows,
            program: "claude-agent-acp".to_owned(),
            args: Vec::new(),
            path: path.clone(),
            home: String::new(),
        };
        assert_eq!(
            half_installed_on(&path, &[claude]),
            [AgentKind::Cursor],
            "the adapter is there, so Claude Code is whole"
        );
        assert_eq!(
            half_installed_on(&path, &[]),
            [AgentKind::Cursor, AgentKind::Claude]
        );
    }
}
