//! Hides secrets before anything is handed to an AI.

use std::sync::LazyLock;

use regex::Regex;

/// What a hidden value is replaced with.
pub const HIDDEN: &str = "‹hidden›";

/// Values shorter than this are not hidden by value: `1` or `on` would
/// blank out half the prompt.
const MIN_SECRET_LEN: usize = 4;

/// Replaces secret values, credentials in headers and token-like strings.
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    /// Longest first, so a secret that contains another is hidden whole.
    secrets: Vec<String>,
}

static BEARER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(bearer|basic|digest|negotiate|ntlm)\s+[A-Za-z0-9._~+/=-]{6,}").unwrap()
});
static JWT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\beyJ[A-Za-z0-9_-]{4,}\.[A-Za-z0-9_-]{4,}\.[A-Za-z0-9_-]*").unwrap()
});
/// `"password": "x"`, `password=x`, `token: x` in JSON, forms, query strings and YAML.
static KEY_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(["']?[A-Za-z0-9_.-]*(?:password|passwd|pwd|secret|token|api[_-]?key|apikey|client[_-]?secret|session[_-]?id|private[_-]?key|credentials?)["']?\s*[:=]\s*["']?)([^"'&\s,}{\]\[]{1,})"#,
    )
    .unwrap()
});
/// `https://user:password@host`.
static URL_USERINFO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b([a-z][a-z0-9+.-]*://)[^/\s:@]+:[^/\s@]+@").unwrap());

impl Redactor {
    /// `secrets` are values to hide wherever they appear, like the values
    /// of an environment's `$secrets`.
    pub fn new(secrets: impl IntoIterator<Item = String>) -> Self {
        let mut secrets: Vec<String> = secrets
            .into_iter()
            .map(|s| s.trim().to_owned())
            .filter(|s| s.chars().count() >= MIN_SECRET_LEN)
            .collect();
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        secrets.dedup();
        Self { secrets }
    }

    /// Hides every secret value and anything that looks like a credential.
    pub fn text(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for secret in &self.secrets {
            if out.contains(secret.as_str()) {
                out = out.replace(secret.as_str(), HIDDEN);
            }
        }
        let out = URL_USERINFO.replace_all(&out, format!("${{1}}{HIDDEN}@"));
        let out = BEARER.replace_all(&out, format!("${{1}} {HIDDEN}"));
        let out = JWT.replace_all(&out, HIDDEN);
        let out = KEY_VALUE.replace_all(&out, |c: &regex::Captures| {
            // `{{password}}` is a template, not a value; keep it readable.
            if c[2].starts_with(HIDDEN) {
                c[0].to_owned()
            } else {
                format!("{}{HIDDEN}", &c[1])
            }
        });
        out.into_owned()
    }

    /// A header value, hidden whole when the header carries credentials.
    pub fn header(&self, name: &str, value: &str) -> String {
        if is_secret_header(name) {
            // Keep the scheme: `Bearer` sent where `Basic` is expected is
            // a common cause of a 401, and says nothing secret.
            match value.trim().split_once(' ') {
                Some((scheme, _)) if is_auth_scheme(scheme) => format!("{scheme} {HIDDEN}"),
                _ => HIDDEN.to_owned(),
            }
        } else {
            self.text(value)
        }
    }
}

fn is_auth_scheme(word: &str) -> bool {
    [
        "bearer",
        "basic",
        "digest",
        "negotiate",
        "ntlm",
        "token",
        "apikey",
        "aws4-hmac-sha256",
    ]
    .contains(&word.to_ascii_lowercase().as_str())
}

/// Headers whose value is a credential whatever it looks like.
pub fn is_secret_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "authorization" | "proxy-authorization" | "cookie" | "set-cookie"
    ) || [
        "token", "secret", "api-key", "apikey", "password", "session", "auth",
    ]
    .iter()
    .any(|word| name.contains(word))
}

/// A variable name that likely holds a secret, to hide its value too.
pub fn is_secret_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "password",
        "passwd",
        "secret",
        "token",
        "apikey",
        "api_key",
        "api-key",
        "credential",
        "private",
    ]
    .iter()
    .any(|word| name.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_known_secrets_longest_first() {
        let r = Redactor::new(["abcd".to_owned(), "abcdef12".to_owned(), "no".to_owned()]);
        assert_eq!(
            r.text("x abcdef12 y abcd z no"),
            "x ‹hidden› y ‹hidden› z no"
        );
    }

    #[test]
    fn hides_credentials_by_shape() {
        let r = Redactor::default();
        assert_eq!(
            r.text("Authorization: Bearer abc.def-123"),
            "Authorization: Bearer ‹hidden›"
        );
        assert_eq!(
            r.text(r#"{"user":"demo","password":"hunter22","access_token":"zzz"}"#),
            r#"{"user":"demo","password":"‹hidden›","access_token":"‹hidden›"}"#
        );
        assert_eq!(
            r.text("/login?user=a&api_key=k123&x=1"),
            "/login?user=a&api_key=‹hidden›&x=1"
        );
        assert_eq!(r.text("https://bob:pw@host/x"), "https://‹hidden›@host/x");
        assert_eq!(
            r.text("got eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig_part here"),
            "got ‹hidden› here"
        );
    }

    #[test]
    fn keeps_templates_and_plain_text() {
        let r = Redactor::default();
        assert_eq!(r.text("password: {{password}}"), "password: {{password}}");
        assert_eq!(
            r.text("password: '{{password}}'"),
            "password: '{{password}}'"
        );
        assert_eq!(r.text("password: 'hunter22'"), "password: '‹hidden›'");
        assert_eq!(
            r.text("status 401, expected 200"),
            "status 401, expected 200"
        );
    }

    #[test]
    fn hides_credential_headers_whole() {
        let r = Redactor::default();
        assert_eq!(r.header("Cookie", "a=1"), HIDDEN);
        assert_eq!(
            r.header("Authorization", "Bearer tok-demo"),
            "Bearer ‹hidden›"
        );
        assert_eq!(r.header("Authorization", "tok-demo"), HIDDEN);
        assert_eq!(r.header("X-Api-Key", "k"), HIDDEN);
        assert_eq!(r.header("Accept", "application/json"), "application/json");
    }
}
