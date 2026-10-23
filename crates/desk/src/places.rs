//! The places a person shares: their machine, and the folders they picked to add. Kept in
//! `settings.json` with the person's other settings.

use std::path::Path;

use player_two_core::api::{PlaceInfo, PlaceKind};
use player_two_core::event::{Mode, RunsIn};
use player_two_core::names::PlaceName;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::system;

/// The person's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// The mode the person's own tags run at.
    #[serde(default = "ask")]
    pub default_mode: Mode,
    /// Where `@me` runs by default: Windows, or a WSL distro.
    #[serde(default = "windows")]
    pub machine_runs_in: RunsIn,
    /// The checkouts shared, by name.
    #[serde(default)]
    pub folders: Vec<Folder>,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Folder {
    /// The name it is tagged by: `@me/name`.
    pub name: PlaceName,
    /// Where it is, as a Windows path, or a Linux one inside its distro.
    pub path: String,
    /// Which system it is on.
    pub runs_in: RunsIn,
}

impl Settings {
    /// What the hub lists of the person's places: the machine, and each folder by name.
    #[must_use]
    pub fn advertised(&self) -> Vec<PlaceInfo> {
        let machine = PlaceInfo {
            name: PlaceName::machine(),
            kind: PlaceKind::Machine,
        };
        let repos = self.folders.iter().map(|folder| PlaceInfo {
            name: folder.name.clone(),
            kind: PlaceKind::Repo,
        });
        std::iter::once(machine).chain(repos).collect()
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

    use super::{Settings, located, name_for, name_from_remote, quoted};

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
    fn the_machine_comes_first_in_what_is_advertised() {
        let settings = Settings::default();
        let advertised = settings.advertised();
        assert_eq!(advertised.len(), 1);
        assert!(advertised[0].name.is_machine());
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
