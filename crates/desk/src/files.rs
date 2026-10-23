//! What the desk keeps on disk, in the app's data folder: who this machine joined as, and the
//! person's settings.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use player_two_core::names::PersonName;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Who this machine joined the hub as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// The hub's address.
    pub hub: String,
    /// The name joined under.
    pub name: PersonName,
    /// The personal token from joining.
    pub token: String,
}

/// The desk's folder.
#[derive(Debug, Clone)]
pub struct Files {
    folder: PathBuf,
}

impl Files {
    /// The files in `folder`, which is made when first written to.
    #[must_use]
    pub fn new(folder: PathBuf) -> Self {
        Self { folder }
    }

    /// Who this machine joined as, if it has.
    pub fn identity(&self) -> anyhow::Result<Option<Identity>> {
        self.read("identity.json")
    }

    /// Records who this machine joined as.
    pub fn save_identity(&self, identity: &Identity) -> anyhow::Result<()> {
        self.write("identity.json", identity)
    }

    /// Reads a JSON file of the folder; `None` when it is not there.
    pub fn read<T: DeserializeOwned>(&self, name: &str) -> anyhow::Result<Option<T>> {
        let path = self.folder.join(name);
        match fs::read(&path) {
            Ok(bytes) => {
                Ok(Some(serde_json::from_slice(&bytes).with_context(|| {
                    format!("{} cannot be read", path.display())
                })?))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    /// Writes a JSON file of the folder, whole or not at all.
    pub fn write<T: Serialize>(&self, name: &str, value: &T) -> anyhow::Result<()> {
        fs::create_dir_all(&self.folder)
            .with_context(|| format!("cannot make {}", self.folder.display()))?;
        let path = self.folder.join(name);
        let partial = self.folder.join(format!("{name}.partial"));
        fs::write(&partial, serde_json::to_vec_pretty(value)?)
            .with_context(|| format!("cannot write {}", partial.display()))?;
        replace(&partial, &path)
    }
}

fn replace(from: &Path, to: &Path) -> anyhow::Result<()> {
    fs::rename(from, to).with_context(|| format!("cannot write {}", to.display()))
}

#[cfg(test)]
mod tests {
    use player_two_core::names::PersonName;

    use super::{Files, Identity};

    #[test]
    fn an_identity_is_kept() {
        let folder = tempfile::tempdir().expect("a folder");
        let files = Files::new(folder.path().join("player-two"));
        assert_eq!(files.identity().ok(), Some(None));
        let identity = Identity {
            hub: "https://hub.example".to_owned(),
            name: PersonName::new("alice").expect("a name"),
            token: "t".to_owned(),
        };
        files.save_identity(&identity).expect("saved");
        assert_eq!(files.identity().ok(), Some(Some(identity)));
    }
}
