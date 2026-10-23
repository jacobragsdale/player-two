//! What crosses between the window and the desk: a request, and its response.
//!
//! These are the only types the webview sees. A test writes them to `ui/src/desk.ts`, so the
//! window is typed by what is here and never by hand.

use player_two_core::names::PersonName;
use serde::{Deserialize, Serialize};
use specta::Type;

/// Something the window asks of the desk.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Asks the desk to say who it is.
    Hello,
    /// Asks whether this machine has joined, and what name to offer if not.
    Setup,
    /// Joins the hub in a join code under a name.
    Join {
        /// The join code, as pasted.
        code: String,
        /// The name to join under.
        name: String,
    },
}

/// The desk's answer to one [`Request`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// The answer to [`Request::Hello`].
    Hello {
        /// The version the hub, the app and the desk all share.
        version: String,
    },
    /// The answer to [`Request::Setup`].
    Setup {
        /// The name this machine joined under, once it has.
        joined: Option<PersonName>,
        /// The name to offer, from the Windows user name.
        suggested_name: Option<String>,
    },
    /// The request was done, and there is nothing more to say.
    Done,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use specta::Types;
    use specta_typescript::Typescript;

    use super::{Request, Response};

    const HEADER: &str = "// Written by the desk's tests from crates/desk/src/ui.rs.";

    /// Rewrites `ui/src/desk.ts` when it is out of date, and fails so that the change is seen.
    #[test]
    fn the_window_has_the_desks_types() {
        let types = Types::default()
            .register::<Request>()
            .register::<Response>();
        let written = Typescript::default()
            .header(HEADER)
            .export(&types, specta_serde::Format)
            .expect("the desk's types can be written as TypeScript");
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/desk.ts");
        let found = fs::read_to_string(&path).unwrap_or_default();
        if found != written {
            fs::write(&path, &written).expect("ui/src/desk.ts can be written");
            panic!("ui/src/desk.ts was out of date; it has been rewritten, so commit it");
        }
    }
}
