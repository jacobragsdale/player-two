//! Hiding credentials in what a run shows the chat: the password in a URL's `user:password@`, and
//! the value after `password=`, `token=` or `Authorization:`, in any case. It is narrow on
//! purpose: anything broader waits for the owner's yes.

/// What stands in for a hidden value.
const HIDDEN: &str = "****";

/// `text` with its credentials hidden.
#[must_use]
pub fn mask(text: &str) -> String {
    let text = urls(text);
    let text = after(&text, "authorization", ':', |c| c == '\n' || c == '\r');
    let text = after(&text, "password", '=', ends_value);
    after(&text, "token", '=', ends_value)
}

/// Whether a character ends a `key=value` value.
fn ends_value(c: char) -> bool {
    c.is_whitespace() || matches!(c, '&' | ';' | ',' | '"' | '\'')
}

/// Hides the password of each URL's `user:password@`.
fn urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (before, after) = rest.split_at(at + 3);
        out.push_str(before);
        let authority = after
            .find(|c: char| c.is_whitespace() || matches!(c, '/' | '?' | '#' | '"' | '\''))
            .unwrap_or(after.len());
        let (host, tail) = after.split_at(authority);
        match host
            .rfind('@')
            .and_then(|end| host[..end].find(':').map(|colon| (colon, end)))
        {
            Some((colon, end)) => {
                out.push_str(&host[..=colon]);
                out.push_str(HIDDEN);
                out.push_str(&host[end..]);
            }
            None => out.push_str(host),
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

/// Hides the value after each `key`, in any case, that is followed by `separator`, with spaces
/// allowed around it. The value runs until `ends`.
fn after(text: &str, key: &str, separator: char, ends: impl Fn(char) -> bool) -> String {
    // ASCII lowercasing keeps every byte where it was, so offsets in one are offsets in the other.
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    while let Some(found) = lower[from..].find(key) {
        let key_end = from + found + key.len();
        let spaced = |at: usize| {
            at + text[at..]
                .find(|c: char| c != ' ' && c != '\t')
                .unwrap_or(text.len() - at)
        };
        let at = spaced(key_end);
        if !text[at..].starts_with(separator) {
            out.push_str(&text[from..key_end]);
            from = key_end;
            continue;
        }
        let start = spaced(at + separator.len_utf8());
        let end = start + text[start..].find(&ends).unwrap_or(text.len() - start);
        out.push_str(&text[from..start]);
        if end > start {
            out.push_str(HIDDEN);
        }
        from = end;
    }
    out.push_str(&text[from..]);
    out
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn a_urls_password_is_hidden() {
        assert_eq!(
            mask("proxy = http://jacob:hunter2@proxy.example:8080/path"),
            "proxy = http://jacob:****@proxy.example:8080/path"
        );
        assert_eq!(
            mask("git clone https://user:p@ss:word@dev.example/repo.git and http://plain.example"),
            "git clone https://user:****@dev.example/repo.git and http://plain.example"
        );
        assert_eq!(
            mask("https://jacob@dev.example/x"),
            "https://jacob@dev.example/x",
            "a user without a password hides nothing"
        );
    }

    #[test]
    fn values_after_their_keys_are_hidden_in_any_case() {
        assert_eq!(
            mask("PROXY_PASSWORD=s3cret\nexport GITHUB_TOKEN=ghp_abc123 done"),
            "PROXY_PASSWORD=****\nexport GITHUB_TOKEN=**** done"
        );
        assert_eq!(mask("password = hunter2; next"), "password = ****; next");
        assert_eq!(mask("GET /?a=1&token=xyz&b=2"), "GET /?a=1&token=****&b=2");
        assert_eq!(
            mask("Authorization: Basic YWxhZGRpbjpvcGVu\nAccept: */*"),
            "Authorization: ****\nAccept: */*"
        );
    }

    #[test]
    fn ordinary_text_is_left_alone() {
        for text in [
            "the password is wrong, and tokens = 5",
            "git@github.com:jacob/player-two.git",
            "pip config set global.proxy http://proxy:8080",
            "authorization needed",
            "password=",
            "émigré passwörd",
        ] {
            assert_eq!(mask(text), text);
        }
    }
}
