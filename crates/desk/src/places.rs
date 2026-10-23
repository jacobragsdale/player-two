//! The places a person shares: their machine, and the folders they picked to add. Kept in
//! `settings.json` with the person's other settings.

use std::collections::BTreeMap;
use std::path::Path;

use player_two_core::api::{PlaceInfo, PlaceKind};
use player_two_core::event::{Mode, RunsIn};
use player_two_core::names::PlaceName;
use serde::{Deserialize, Serialize};

use crate::{agents, system};

/// The person's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// The mode the person's own tags run at.
    #[serde(default = "ask")]
    pub default_mode: Mode,
    /// Where `@me` runs by default: Windows, or a WSL distro.
    #[serde(default = "windows")]
    pub machine_runs_in: RunsIn,
    /// The folders shared, by name.
    #[serde(default)]
    pub folders: Vec<Folder>,
    /// The agent chosen for each system that has more than one. Elsewhere the most preferred
    /// runs, which is Cursor when it is there.
    #[serde(default)]
    pub agents: BTreeMap<RunsIn, agents::AgentKind>,
    /// An agent of the person's own choosing, run on this system in place of any found: a
    /// program and its arguments, speaking ACP.
    #[serde(default)]
    pub custom_agent: Option<Vec<String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_mode: ask(),
            machine_runs_in: windows(),
            folders: Vec::new(),
            agents: BTreeMap::new(),
            custom_agent: None,
        }
    }
}

fn ask() -> Mode {
    Mode::Ask
}

fn windows() -> RunsIn {
    RunsIn::Windows
}

/// One shared folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    /// The name it is tagged by: `@me/name`.
    pub name: PlaceName,
    /// Where it is, as a Windows path, or a Linux one inside its distro.
    pub path: String,
    /// Which system it is on.
    pub runs_in: RunsIn,
    /// The names it had before it was renamed, which still reach it from older requests and
    /// threads. No other folder is given one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub former: Vec<PlaceName>,
}

/// What the desk found when it last looked, kept in `found.json`: the agents on each system, and
/// the shared folders that were not there. The places advertised follow from it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Looked {
    /// The agents found, on Windows and in each WSL distro.
    pub agents: Vec<agents::Found>,
    /// The shared folders not there, by their system and path.
    pub missing: Vec<(RunsIn, String)>,
}

impl Looked {
    /// Why a shared folder cannot run now, if it cannot: it is gone, or there is no agent on its
    /// system.
    #[must_use]
    pub fn unavailable(&self, folder: &Folder) -> Option<String> {
        if self
            .missing
            .iter()
            .any(|(runs_in, path)| runs_in == &folder.runs_in && path == &folder.path)
        {
            return Some("the folder is not there any more".to_owned());
        }
        if self
            .agents
            .iter()
            .any(|agent| agent.runs_in == folder.runs_in)
        {
            return None;
        }
        Some(match &folder.runs_in {
            RunsIn::Windows => "there is no agent on Windows".to_owned(),
            RunsIn::Wsl(distro) => format!("there is no agent in {distro}"),
        })
    }
}

impl Settings {
    /// The shared folder a place names, by its name or a former one.
    #[must_use]
    pub fn folder(&self, place: &PlaceName) -> Option<&Folder> {
        self.folders
            .iter()
            .find(|folder| &folder.name == place)
            .or_else(|| {
                self.folders
                    .iter()
                    .find(|folder| folder.former.contains(place))
            })
    }

    /// The name a place goes by now: a renamed folder's new name, and any other place's own.
    #[must_use]
    pub fn current(&self, place: &PlaceName) -> PlaceName {
        self.folder(place)
            .map_or_else(|| place.clone(), |folder| folder.name.clone())
    }

    /// Every name a folder has or had, which a new folder cannot take.
    #[must_use]
    pub fn taken(&self) -> Vec<PlaceName> {
        self.folders
            .iter()
            .flat_map(|folder| std::iter::once(&folder.name).chain(&folder.former))
            .cloned()
            .collect()
    }

    /// Renames the folder now called `name` to `to`. Its old name is kept as a former one; a name
    /// it had before may be taken back.
    pub fn rename(&mut self, name: &PlaceName, to: &str) -> anyhow::Result<()> {
        let to = PlaceName::new(to.trim()).map_err(|_| {
            anyhow::anyhow!(
                "a name is lowercase letters, digits, ., _ or -, up to 64, starting with a letter or digit"
            )
        })?;
        anyhow::ensure!(!to.is_machine(), "machine is the name of your computer");
        let taken_elsewhere = self
            .folders
            .iter()
            .filter(|folder| &folder.name != name)
            .any(|folder| folder.name == to || folder.former.contains(&to));
        anyhow::ensure!(!taken_elsewhere, "another folder is or was called {to}");
        let folder = self
            .folders
            .iter_mut()
            .find(|folder| &folder.name == name)
            .ok_or_else(|| anyhow::anyhow!("{name} is not shared"))?;
        if folder.name == to {
            return Ok(());
        }
        folder.former.retain(|former| former != &to);
        folder.former.push(folder.name.clone());
        folder.name = to;
        Ok(())
    }

    /// What the hub lists of the person's places: the places that can run. That is the machine
    /// when an agent was found anywhere, and each folder that is there with an agent on its
    /// system.
    #[must_use]
    pub fn advertised(&self, looked: &Looked) -> Vec<PlaceInfo> {
        if looked.agents.is_empty() {
            return Vec::new();
        }
        let machine = PlaceInfo {
            name: PlaceName::machine(),
            kind: PlaceKind::Machine,
        };
        let folders = self
            .folders
            .iter()
            .filter(|folder| looked.unavailable(folder).is_none())
            .map(|folder| PlaceInfo {
                name: folder.name.clone(),
                kind: PlaceKind::Repo,
            });
        std::iter::once(machine).chain(folders).collect()
    }
}

/// A checkout's name from its `origin` remote: the remote's last part, without `.git`,
/// lowercased, with anything a place name cannot hold turned into `-`.
#[must_use]
pub fn name_from_remote(remote: &str) -> Option<String> {
    let trimmed = remote.trim().trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':', '\\']).next()?;
    let bare = last.strip_suffix(".git").unwrap_or(last);
    let name = tidy(bare);
    (!name.is_empty()).then_some(name)
}

/// Lowercases a name and turns anything a place name cannot hold into `-`.
fn tidy(name: &str) -> String {
    let lowered: String = name
        .chars()
        .map(|c| {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    lowered
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .chars()
        .take(60)
        .collect()
}

/// A name for a new folder: from a git checkout's remote, or else the folder's own name, made unlike `machine`
/// and every name already taken.
pub fn name_for(
    path: &str,
    remote: Option<&str>,
    taken: &[PlaceName],
) -> anyhow::Result<PlaceName> {
    let folder = path
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    let base = remote
        .and_then(name_from_remote)
        .unwrap_or_else(|| tidy(folder));
    anyhow::ensure!(!base.is_empty(), "{path} has no name to share it under");
    let base = if base == PlaceName::MACHINE {
        format!("{base}-repo")
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut number = 2;
    while taken.iter().any(|name| name.as_str() == candidate) {
        candidate = format!("{base}-{number}");
        number += 1;
    }
    Ok(PlaceName::new(candidate)?)
}

/// A path quoted for bash: in single quotes, with each `'` closed, escaped and reopened.
#[must_use]
pub fn quoted(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

/// The `origin` remote of a git checkout, if git can say.
pub async fn remote_of(path: &str, runs_in: &RunsIn) -> Option<String> {
    let command = match runs_in {
        RunsIn::Windows => {
            let mut git = system::command("git");
            git.args(["-C", path, "remote", "get-url", "origin"]);
            git
        }
        RunsIn::Wsl(distro) => system::in_wsl(
            distro,
            &format!("git -C {} remote get-url origin", quoted(path)),
        ),
    };
    system::output(command)
        .await
        .map(|remote| remote.trim().to_owned())
        .filter(|remote| !remote.is_empty())
}

/// Whether a folder is there.
pub async fn exists(path: &str, runs_in: &RunsIn) -> bool {
    match runs_in {
        RunsIn::Windows => Path::new(path).is_dir(),
        RunsIn::Wsl(distro) => system::output(system::in_wsl(
            distro,
            &format!("test -d {} && echo yes", quoted(path)),
        ))
        .await
        .is_some_and(|said| said.trim() == "yes"),
    }
}

/// Whether a checkout has changes not committed, as `git status` sees it. A checkout git cannot
/// read counts as clean.
pub async fn dirty(path: &str, runs_in: &RunsIn) -> bool {
    let command = match runs_in {
        RunsIn::Windows => {
            let mut git = system::command("git");
            git.args(["-C", path, "status", "--porcelain"]);
            git
        }
        RunsIn::Wsl(distro) => system::in_wsl(
            distro,
            &format!("git -C {} status --porcelain", quoted(path)),
        ),
    };
    system::output(command)
        .await
        .is_some_and(|changes| !changes.trim().is_empty())
}

/// Where a folder picked with the Windows folder dialog is. A path under `\\wsl.localhost\<distro>\`
/// or `\\wsl$\<distro>\` is in that distro, and the rest is its Linux path; anything else is on
/// Windows.
#[must_use]
pub fn located(picked: &str) -> (String, RunsIn) {
    let bare = picked
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .unwrap_or_else(|| picked.to_owned());
    let unc = bare.replace('/', "\\");
    let in_wsl = unc.strip_prefix(r"\\").and_then(|rest| {
        let (host, rest) = rest.split_once('\\')?;
        let wsl = host.eq_ignore_ascii_case("wsl.localhost") || host.eq_ignore_ascii_case("wsl$");
        let (distro, inside) = rest.split_once('\\').unwrap_or((rest, ""));
        (wsl && !distro.is_empty()).then(|| (distro.to_owned(), inside.to_owned()))
    });
    match in_wsl {
        Some((distro, inside)) => {
            let path = format!("/{}", inside.trim_matches('\\').replace('\\', "/"));
            (path, RunsIn::Wsl(distro))
        }
        None => (picked.to_owned(), RunsIn::Windows),
    }
}

#[cfg(test)]
mod tests {
    use player_two_core::event::RunsIn;
    use player_two_core::names::PlaceName;

    use super::{Folder, Looked, Settings, located, name_for, name_from_remote, quoted};
    use crate::agents;

    #[test]
    fn a_checkout_is_named_by_its_remote() {
        let name = |remote: &str| name_from_remote(remote);
        assert_eq!(
            name("https://dev.azure.com/acme/Payments/_git/Uploader"),
            Some("uploader".to_owned())
        );
        assert_eq!(
            name("git@ssh.dev.azure.com:v3/acme/payments/billing-api"),
            Some("billing-api".to_owned())
        );
        assert_eq!(
            name("git@github.com:jacob/player-two.git"),
            Some("player-two".to_owned())
        );
        assert_eq!(
            name("https://github.com/jacob/My Repo.git/"),
            Some("my-repo".to_owned())
        );
        assert_eq!(name(""), None);
    }

    #[test]
    fn names_are_kept_apart() {
        let taken = [PlaceName::new("uploader").expect("a name")];
        let name = |path: &str, remote: Option<&str>| {
            name_for(path, remote, &taken).map(String::from).ok()
        };
        assert_eq!(
            name("C:\\dev\\uploader", Some("https://x/acme/uploader.git")),
            Some("uploader-2".to_owned())
        );
        assert_eq!(name("/home/bob/dev/Notes", None), Some("notes".to_owned()));
        assert_eq!(
            name("C:\\dev\\machine", None),
            Some("machine-repo".to_owned())
        );
        assert_eq!(name("C:\\dev\\___", None), None);
    }

    #[test]
    fn only_places_that_can_run_are_advertised() {
        let place = |name: &str| PlaceName::new(name).expect("a name");
        let ubuntu = RunsIn::Wsl("Ubuntu".to_owned());
        let folder = |name: &str, runs_in: &RunsIn| Folder {
            name: place(name),
            path: format!("/{name}"),
            runs_in: runs_in.clone(),
            former: Vec::new(),
        };
        let settings = Settings {
            folders: vec![
                folder("here", &RunsIn::Windows),
                folder("gone", &RunsIn::Windows),
                folder("linux", &ubuntu),
            ],
            ..Settings::default()
        };
        let agent = |runs_in: &RunsIn| agents::Found {
            kind: agents::AgentKind::Cursor,
            runs_in: runs_in.clone(),
            program: "agent".to_owned(),
            args: Vec::new(),
            path: String::new(),
            home: String::new(),
        };
        let names = |looked: &Looked| -> Vec<String> {
            settings
                .advertised(looked)
                .into_iter()
                .map(|info| info.name.to_string())
                .collect()
        };
        assert!(names(&Looked::default()).is_empty(), "no agent, no places");
        let mut looked = Looked {
            agents: vec![agent(&RunsIn::Windows)],
            missing: vec![(RunsIn::Windows, "/gone".to_owned())],
        };
        assert_eq!(names(&looked), ["machine", "here"]);
        assert_eq!(
            looked.unavailable(&settings.folders[1]).as_deref(),
            Some("the folder is not there any more")
        );
        assert_eq!(
            looked.unavailable(&settings.folders[2]).as_deref(),
            Some("there is no agent in Ubuntu")
        );
        looked.agents.push(agent(&ubuntu));
        assert_eq!(names(&looked), ["machine", "here", "linux"]);
        looked.agents.retain(|found| found.runs_in == ubuntu);
        assert_eq!(names(&looked), ["machine", "linux"]);
    }

    #[test]
    fn a_renamed_folder_answers_to_its_former_names() {
        let place = |name: &str| PlaceName::new(name).expect("a name");
        let folder = |name: &str| Folder {
            name: place(name),
            path: format!("C:\\dev\\{name}"),
            runs_in: RunsIn::Windows,
            former: Vec::new(),
        };
        let mut settings = Settings {
            folders: vec![folder("uploader"), folder("uploader-2")],
            ..Settings::default()
        };
        settings
            .rename(&place("uploader-2"), " uploader-old ")
            .expect("renamed");
        assert_eq!(settings.folders[1].name.as_str(), "uploader-old");
        assert_eq!(settings.folders[1].former, [place("uploader-2")]);
        assert_eq!(
            settings
                .folder(&place("uploader-2"))
                .map(|found| &found.path),
            Some(&"C:\\dev\\uploader-2".to_owned())
        );
        assert_eq!(
            settings.current(&place("uploader-2")),
            place("uploader-old")
        );
        assert_eq!(
            settings.current(&PlaceName::machine()),
            PlaceName::machine()
        );
        for refused in ["uploader-2", "uploader-old", "machine", "Not A Name"] {
            assert!(
                settings.rename(&place("uploader"), refused).is_err(),
                "{refused} is refused"
            );
        }
        settings
            .rename(&place("uploader-old"), "uploader-2")
            .expect("a former name is taken back");
        assert_eq!(settings.folders[1].former, [place("uploader-old")]);
        assert_eq!(
            settings.taken(),
            [
                place("uploader"),
                place("uploader-2"),
                place("uploader-old")
            ]
        );
    }

    #[test]
    fn a_folder_picked_through_wsl_is_in_its_distro() {
        let wsl = |distro: &str| RunsIn::Wsl(distro.to_owned());
        assert_eq!(
            located(r"\\wsl.localhost\Ubuntu\home\bob\proxy-scripts"),
            ("/home/bob/proxy-scripts".to_owned(), wsl("Ubuntu"))
        );
        assert_eq!(
            located(r"\\wsl$\Debian\srv\app\"),
            ("/srv/app".to_owned(), wsl("Debian"))
        );
        assert_eq!(
            located(r"\\?\UNC\WSL.LOCALHOST\Ubuntu-24.04\home"),
            ("/home".to_owned(), wsl("Ubuntu-24.04"))
        );
        assert_eq!(
            located(r"\\wsl.localhost\Ubuntu"),
            ("/".to_owned(), wsl("Ubuntu"))
        );
        assert_eq!(
            located(r"C:\Users\bob\tools"),
            (r"C:\Users\bob\tools".to_owned(), RunsIn::Windows)
        );
        assert_eq!(
            located(r"\\fileserver\share\team"),
            (r"\\fileserver\share\team".to_owned(), RunsIn::Windows)
        );
        assert_eq!(
            located("/tmp/folder"),
            ("/tmp/folder".to_owned(), RunsIn::Windows)
        );
    }

    #[test]
    fn paths_are_quoted_for_bash() {
        assert_eq!(quoted("/home/bob/it's here"), "'/home/bob/it'\\''s here'");
    }
}
