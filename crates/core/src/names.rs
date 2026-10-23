//! The names people, channels, chats and places go by, each checked when it is made.

use std::fmt;

use serde::{Deserialize, Serialize};
use specta::Type;

/// Why a name was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadName {
    what: &'static str,
    given: String,
}

impl fmt::Display for BadName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} is not a valid {}", self.given, self.what)
    }
}

impl std::error::Error for BadName {}

fn bad(what: &'static str, given: &str) -> BadName {
    BadName {
        what,
        given: given.to_owned(),
    }
}

fn is_lower_alnum(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit()
}

/// Defines a checked string newtype: its accessors, conversions and serde through `String`.
macro_rules! checked_name {
    ($name:ident, $what:literal, $check:expr) => {
        impl $name {
            /// Checks `name` and wraps it.
            pub fn new(name: impl Into<String>) -> Result<Self, BadName> {
                let name = name.into();
                let check: fn(&str) -> bool = $check;
                if check(&name) {
                    Ok(Self(name))
                } else {
                    Err(bad($what, &name))
                }
            }

            /// The name as written.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = BadName;

            fn try_from(name: String) -> Result<Self, BadName> {
                Self::new(name)
            }
        }

        impl From<$name> for String {
            fn from(name: $name) -> String {
                name.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

/// A person's name on the hub: a lowercase letter, then up to 31 lowercase letters, digits or `-`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
#[serde(try_from = "String", into = "String")]
#[specta(transparent)]
pub struct PersonName(String);

checked_name!(PersonName, "person's name", |name| {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| is_lower_alnum(c) || c == '-')
        && name.len() <= 32
});

impl PersonName {
    /// The name to offer someone whose Windows user name is `user`: lowercased, with anything
    /// else turned into `-`. `None` when nothing usable is left.
    #[must_use]
    pub fn from_user_name(user: &str) -> Option<Self> {
        let lowered: String = user
            .trim()
            .chars()
            .map(|c| {
                let c = c.to_ascii_lowercase();
                if is_lower_alnum(c) { c } else { '-' }
            })
            .skip_while(|c| !c.is_ascii_lowercase())
            .take(32)
            .collect();
        Self::new(lowered.trim_end_matches('-')).ok()
    }
}

/// A channel's name, without its `#`: 1 to 32 lowercase letters, digits or `-`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
#[serde(try_from = "String", into = "String")]
#[specta(transparent)]
pub struct ChannelName(String);

checked_name!(ChannelName, "channel name", |name| {
    !name.is_empty() && name.len() <= 32 && name.chars().all(|c| is_lower_alnum(c) || c == '-')
});

/// Where an agent can run for someone: `machine`, or one of their shared checkouts, named by up
/// to 64 lowercase letters, digits, `.`, `_` or `-`, starting with a letter or digit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
#[serde(try_from = "String", into = "String")]
#[specta(transparent)]
pub struct PlaceName(String);

checked_name!(PlaceName, "place name", |name| {
    let mut chars = name.chars();
    chars.next().is_some_and(is_lower_alnum)
        && chars.all(|c| is_lower_alnum(c) || matches!(c, '.' | '_' | '-'))
        && name.len() <= 64
});

impl PlaceName {
    /// The name of a person's machine as a place.
    pub const MACHINE: &str = "machine";

    /// A person's machine.
    #[must_use]
    pub fn machine() -> Self {
        Self(Self::MACHINE.to_owned())
    }

    /// Whether this is a machine rather than a checkout.
    #[must_use]
    pub fn is_machine(&self) -> bool {
        self.0 == Self::MACHINE
    }
}

/// A chat: a channel, written `#name`, or a direct chat between two people, written with their
/// names in sorted order joined by `+`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
#[serde(try_from = "String", into = "String")]
#[specta(type = String)]
pub enum ChatId {
    /// A channel.
    Channel(ChannelName),
    /// A direct chat. The first name sorts before the second.
    Direct(PersonName, PersonName),
}

impl ChatId {
    /// The direct chat between two different people.
    pub fn direct(one: PersonName, other: PersonName) -> Result<Self, BadName> {
        match one.cmp(&other) {
            std::cmp::Ordering::Less => Ok(Self::Direct(one, other)),
            std::cmp::Ordering::Greater => Ok(Self::Direct(other, one)),
            std::cmp::Ordering::Equal => Err(bad("direct chat", &format!("{one}+{other}"))),
        }
    }

    /// Whether `name` is one of a direct chat's two people. Always false for a channel.
    #[must_use]
    pub fn is_between(&self, name: &PersonName) -> bool {
        matches!(self, Self::Direct(one, other) if one == name || other == name)
    }
}

impl TryFrom<String> for ChatId {
    type Error = BadName;

    fn try_from(id: String) -> Result<Self, BadName> {
        if let Some(channel) = id.strip_prefix('#') {
            return Ok(Self::Channel(ChannelName::new(channel)?));
        }
        let (one, other) = id.split_once('+').ok_or_else(|| bad("chat", &id))?;
        let (one, other) = (PersonName::new(one)?, PersonName::new(other)?);
        if one < other {
            Ok(Self::Direct(one, other))
        } else {
            Err(bad("chat", &id))
        }
    }
}

impl fmt::Display for ChatId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Channel(name) => write!(f, "#{name}"),
            Self::Direct(one, other) => write!(f, "{one}+{other}"),
        }
    }
}

impl From<ChatId> for String {
    fn from(id: ChatId) -> String {
        id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{ChannelName, ChatId, PersonName, PlaceName};

    #[test]
    fn person_names() {
        for good in ["a", "alice", "bob-2", "x23456789012345678901234567890ab"] {
            assert!(PersonName::new(good).is_ok(), "{good} is a name");
        }
        for bad in [
            "",
            "Alice",
            "2bob",
            "-bob",
            "bob smith",
            "hub!",
            "x23456789012345678901234567890abc",
        ] {
            assert!(PersonName::new(bad).is_err(), "{bad} is not a name");
        }
    }

    #[test]
    fn a_name_from_a_windows_user_name() {
        let offer = |user: &str| PersonName::from_user_name(user).map(String::from);
        assert_eq!(offer("Jacob.Ragsdale"), Some("jacob-ragsdale".to_owned()));
        assert_eq!(offer("  ALICE "), Some("alice".to_owned()));
        assert_eq!(offer("2nd_user"), Some("nd-user".to_owned()));
        assert_eq!(offer("bob_"), Some("bob".to_owned()));
        assert_eq!(offer("12345"), None);
        assert_eq!(offer(""), None);
    }

    #[test]
    fn channel_and_place_names() {
        assert!(ChannelName::new("ops-2").is_ok());
        assert!(ChannelName::new("#ops").is_err());
        assert!(ChannelName::new("").is_err());
        assert!(PlaceName::new("uploader").is_ok());
        assert!(PlaceName::new("my.repo_2-x").is_ok());
        assert!(PlaceName::new(".").is_err());
        assert!(PlaceName::new("..").is_err());
        assert!(PlaceName::new("Repo").is_err());
        assert!(PlaceName::machine().is_machine());
        assert!(!PlaceName::new("uploader").is_ok_and(|place| place.is_machine()));
    }

    #[test]
    fn chat_ids() {
        let alice = PersonName::new("alice").expect("a name");
        let bob = PersonName::new("bob").expect("a name");
        let direct = ChatId::direct(bob.clone(), alice.clone()).expect("two people");
        assert_eq!(direct.to_string(), "alice+bob");
        assert!(direct.is_between(&alice) && direct.is_between(&bob));
        assert!(ChatId::direct(bob.clone(), bob).is_err());
        let parse = |id: &str| ChatId::try_from(id.to_owned()).map(|chat| chat.to_string());
        assert_eq!(parse("#ops"), Ok("#ops".to_owned()));
        assert_eq!(parse("alice+bob"), Ok("alice+bob".to_owned()));
        for bad in [
            "bob+alice",
            "alice+alice",
            "alice",
            "#",
            "#Ops",
            "alice+bob+carol",
        ] {
            assert!(parse(bad).is_err(), "{bad} is not a chat");
        }
        let json = serde_json::to_string(&direct).expect("a chat id serializes");
        assert_eq!(json, "\"alice+bob\"");
    }
}
