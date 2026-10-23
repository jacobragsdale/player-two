//! The join code an admin sends someone: the hub's address and the join token in one string.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

const PREFIX: &str = "p2-";

/// What a join code holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinCode {
    /// The hub's address, `http` or `https`, with no slash at the end.
    pub hub: String,
    /// The shared join token.
    pub token: String,
}

/// Why a join code could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadJoinCode;

impl fmt::Display for BadJoinCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("that is not a Player Two join code; ask whoever sent it for a new one")
    }
}

impl std::error::Error for BadJoinCode {}

impl JoinCode {
    /// Writes the code: `p2-` and the unpadded base64url of its JSON.
    #[must_use]
    pub fn encode(&self) -> String {
        let json = serde_json::to_vec(self).unwrap_or_default();
        format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(json))
    }

    /// Reads a code, as pasted: surrounding space is ignored.
    pub fn decode(code: &str) -> Result<Self, BadJoinCode> {
        let encoded = code.trim().strip_prefix(PREFIX).ok_or(BadJoinCode)?;
        let json = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| BadJoinCode)?;
        let code: Self = serde_json::from_slice(&json).map_err(|_| BadJoinCode)?;
        let address = code
            .hub
            .strip_prefix("https://")
            .or(code.hub.strip_prefix("http://"));
        match address {
            Some(rest)
                if !rest.is_empty() && !code.hub.ends_with('/') && !code.token.is_empty() =>
            {
                Ok(code)
            }
            _ => Err(BadJoinCode),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::JoinCode;

    #[test]
    fn a_code_reads_back_as_written() {
        let code = JoinCode {
            hub: "https://hub.example".to_owned(),
            token: "s3cret".to_owned(),
        };
        let written = code.encode();
        assert!(written.starts_with("p2-"));
        assert_eq!(JoinCode::decode(&format!("  {written}\n")), Ok(code));
    }

    #[test]
    fn a_bad_code_is_refused() {
        let encode = |hub: &str, token: &str| {
            JoinCode {
                hub: hub.to_owned(),
                token: token.to_owned(),
            }
            .encode()
        };
        for bad in [
            "hello".to_owned(),
            "p2-!!!".to_owned(),
            encode("ftp://hub", "t"),
            encode("https://hub/", "t"),
            encode("https://", "t"),
            encode("https://hub", ""),
        ] {
            assert!(JoinCode::decode(&bad).is_err(), "{bad} is refused");
        }
    }
}
