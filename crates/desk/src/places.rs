//! The places a person shares: their machine, and the checkouts they chose to add. Kept in
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_mode: ask(),
            machine_runs_in: windows(),
            folders: Vec::new(),
        }
    }
}

fn ask() -> Mode {
    Mode::Ask
}

fn windows() -> RunsIn {
    RunsIn::Windows
}

/// One shared checkout.
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
    /// What the hub lists of the person's places: the machine, and each checkout by name.
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

/// A name for a new checkout: from its remote, or else its folder's name, made unlike `machine`
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

/// The `origin` remote of a checkout, if git can say.
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

/// Checkouts worth offering: the git folders directly under `dev` in the home folder, and under
/// `~/dev` in each WSL distro, that are not shared already.
pub async fn suggestions(settings: &Settings, distros: &[String]) -> Vec<Folder> {
    let mut found: Vec<(String, RunsIn)> = Vec::new();
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let dev = Path::new(&home).join("dev");
        if let Ok(entries) = std::fs::read_dir(&dev) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.join(".git").exists() {
                    found.push((path.to_string_lossy().into_owned(), RunsIn::Windows));
                }
            }
        }
    }
    for distro in distros {
        let script = "for d in ~/dev/*/; do [ -e \"$d.git\" ] && printf '%s\\n' \"${d%/}\"; done";
        if let Some(listed) = system::output(system::in_wsl(distro, script)).await {
            found.extend(
                listed
                    .lines()
                    .filter(|line| !line.is_empty())
                    .map(|line| (line.to_owned(), RunsIn::Wsl(distro.clone()))),
            );
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    let mut taken: Vec<PlaceName> = settings
        .folders
        .iter()
        .map(|folder| folder.name.clone())
        .collect();
    let mut offered = Vec::new();
    for (path, runs_in) in found {
        if settings
            .folders
            .iter()
            .any(|folder| folder.path == path && folder.runs_in == runs_in)
        {
            continue;
        }
        let remote = remote_of(&path, &runs_in).await;
        if let Ok(name) = name_for(&path, remote.as_deref(), &taken) {
            taken.push(name.clone());
            offered.push(Folder {
                name,
                path,
                runs_in,
            });
        }
    }
    offered
}

#[cfg(test)]
mod tests {
    use player_two_core::names::PlaceName;

    use super::{Settings, name_for, name_from_remote, quoted};

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
    fn paths_are_quoted_for_bash() {
        assert_eq!(quoted("/home/bob/it's here"), "'/home/bob/it'\\''s here'");
    }
}
