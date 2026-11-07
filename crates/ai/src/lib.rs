//! AI help for Stormix, through the AI command line the user already has
//! installed and signed in: Claude Code (`claude`) or GitHub Copilot CLI
//! (`copilot`). Stormix holds no API key and opens no connection of its
//! own; the command line uses its own account, proxy and policy.
//!
//! Each question is one non-interactive call with no tools: Stormix puts
//! the facts in the prompt (see [`prompt`]), hidden secrets first (see
//! [`redact`]), and the model only answers.

pub mod prompt;
pub mod redact;

use std::ffi::OsString;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub use redact::Redactor;

/// An AI command line Stormix can use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Provider {
    /// Anthropic's Claude Code, `claude`.
    Claude,
    /// GitHub Copilot CLI, `copilot`.
    Copilot,
}

impl Provider {
    pub const ALL: [Provider; 2] = [Provider::Claude, Provider::Copilot];

    pub fn label(self) -> &'static str {
        match self {
            Provider::Claude => "Claude Code",
            Provider::Copilot => "GitHub Copilot CLI",
        }
    }

    pub fn command(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Copilot => "copilot",
        }
    }

    /// What to run once in a terminal to sign in.
    pub fn sign_in_hint(self) -> &'static str {
        match self {
            Provider::Claude => "Run `claude` once in a terminal and sign in, then try again.",
            Provider::Copilot => "Run `copilot` once in a terminal and use /login, then try again.",
        }
    }

    pub fn install_url(self) -> &'static str {
        match self {
            Provider::Claude => "https://docs.claude.com/en/docs/claude-code/setup",
            Provider::Copilot => {
                "https://docs.github.com/en/copilot/how-tos/set-up/install-copilot-cli"
            }
        }
    }
}

impl std::str::FromStr for Provider {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "claude" | "claude-code" => Ok(Provider::Claude),
            "copilot" | "github-copilot" => Ok(Provider::Copilot),
            other => Err(format!(
                "unknown AI provider `{other}`; use claude or copilot"
            )),
        }
    }
}

/// A provider found on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub provider: Provider,
    /// The program that runs, as found or as the user set it.
    pub path: String,
    /// The first line `--version` printed, like `2.1.0 (Claude Code)`.
    /// `None` when it did not answer.
    pub version: Option<String>,
}

/// How to ask.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AskOptions {
    /// The model to use, as the provider names it. `None` keeps the
    /// provider's own default.
    #[serde(default)]
    pub model: Option<String>,
    /// Seconds to wait for an answer; 120 when unset.
    #[serde(default)]
    pub timeout_s: Option<u32>,
}

/// What came back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    pub provider: Provider,
    pub text: String,
    pub duration_ms: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("{label} was not found. Install it from {url}, or set its path in Settings.", label = .0.label(), url = .0.install_url())]
    NotFound(Provider),
    #[error("{label} could not start: {err}", label = .0.label(), err = .1)]
    Spawn(Provider, std::io::Error),
    #[error("{label} did not answer within {secs} s.", label = .0.label(), secs = .1)]
    Timeout(Provider, u32),
    #[error("Stopped.")]
    Cancelled,
    #[error("{label} is not signed in. {hint}", label = .0.label(), hint = .0.sign_in_hint())]
    NotSignedIn(Provider, String),
    #[error("{label} failed: {message}", label = .0.label(), message = .1)]
    Failed(Provider, String),
}

const DEFAULT_TIMEOUT_S: u32 = 120;
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// Finds each provider: at `paths` when the user set one, else on `PATH`
/// and in the folders installers use, which apps started from the Dock or
/// Start menu often do not have on their `PATH`.
pub async fn detect_all(paths: &[(Provider, String)]) -> Vec<Found> {
    let mut found = Vec::new();
    for provider in Provider::ALL {
        let custom = paths
            .iter()
            .find(|(p, path)| *p == provider && !path.trim().is_empty());
        if let Some(f) = detect(provider, custom.map(|(_, path)| Path::new(path.trim()))).await {
            found.push(f);
        }
    }
    found
}

/// Finds one provider; see [`detect_all`].
pub async fn detect(provider: Provider, custom: Option<&Path>) -> Option<Found> {
    let path = match custom {
        Some(path) => path.is_file().then(|| path.to_path_buf())?,
        None => find_program(provider.command())?,
    };
    let version = version(&path).await;
    Some(Found {
        provider,
        path: path.to_string_lossy().into_owned(),
        version,
    })
}

async fn version(path: &Path) -> Option<String> {
    let mut cmd = command(path);
    cmd.arg("--version").stdin(Stdio::null());
    let out = tokio::time::timeout(VERSION_TIMEOUT, cmd.output())
        .await
        .ok()?
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_owned)
}

/// Asks `found` one question, with no tools, and returns its answer.
/// Resolving `cancel` stops it and kills the process.
pub async fn ask(
    found: &Found,
    prompt: &str,
    options: &AskOptions,
    cancel: impl Future<Output = ()>,
) -> Result<Answer, AiError> {
    let provider = found.provider;
    let started = Instant::now();
    // An empty folder to run in, so the command line sees none of the
    // user's files or project settings.
    let dir = tempfile::Builder::new()
        .prefix("stormix-ai-")
        .tempdir()
        .map_err(|e| AiError::Spawn(provider, e))?;
    let mut cmd = command(Path::new(&found.path));
    cmd.current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let stdin: Option<String> = match provider {
        Provider::Claude => {
            cmd.args(claude_args(options));
            Some(prompt.to_owned())
        }
        Provider::Copilot => {
            // Copilot takes its prompt as an argument only. A long one with
            // line breaks does not survive Windows' `copilot.cmd`, so the
            // facts go in a file it may read and nothing else.
            std::fs::write(dir.path().join(CONTEXT_FILE), prompt)
                .map_err(|e| AiError::Spawn(provider, e))?;
            cmd.args(copilot_args(options, dir.path()));
            None
        }
    };
    let mut child = cmd.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => AiError::NotFound(provider),
        _ => AiError::Spawn(provider, e),
    })?;
    let mut pipe = child.stdin.take();
    let feed = async move {
        if let (Some(pipe), Some(text)) = (pipe.as_mut(), stdin) {
            let _ = pipe.write_all(text.as_bytes()).await;
        }
        drop(pipe);
    };
    let timeout_s = options.timeout_s.unwrap_or(DEFAULT_TIMEOUT_S).max(5);
    let run = async {
        feed.await;
        child.wait_with_output().await
    };
    let output = tokio::select! {
        out = tokio::time::timeout(Duration::from_secs(timeout_s.into()), run) => match out {
            Err(_) => return Err(AiError::Timeout(provider, timeout_s)),
            Ok(out) => out.map_err(|e| AiError::Spawn(provider, e))?,
        },
        () = cancel => return Err(AiError::Cancelled),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = match provider {
        Provider::Claude => claude_answer(&stdout, &stderr)?,
        Provider::Copilot => copilot_answer(output.status.success(), &stdout, &stderr)?,
    };
    drop(dir);
    Ok(Answer {
        provider,
        text,
        duration_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

const CONTEXT_FILE: &str = "stormix-question.md";

fn claude_args(options: &AskOptions) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "-p",
        "--output-format",
        "json",
        // No tools at all: the prompt holds everything it needs.
        "--tools",
        "",
        "--strict-mcp-config",
        "--no-session-persistence",
    ]
    .map(OsString::from)
    .to_vec();
    if let Some(model) = options.model.as_deref().filter(|m| !m.trim().is_empty()) {
        args.extend(["--model".into(), model.trim().into()]);
    }
    args
}

fn copilot_args(options: &AskOptions, dir: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "-p".into(),
        format!(
            "Read the file {CONTEXT_FILE} in the current folder and do what its first paragraphs ask, \
             using only the data in it. Reply with the answer only."
        )
        .into(),
        "-s".into(),
        "--no-color".into(),
        "--no-custom-instructions".into(),
        "--disable-builtin-mcps".into(),
        "--no-ask-user".into(),
        "--no-auto-update".into(),
        "--allow-tool=read".into(),
        "--deny-tool=shell".into(),
        "--deny-tool=write".into(),
        "--deny-tool=url".into(),
        "--deny-tool=memory".into(),
        "-C".into(),
        dir.as_os_str().to_owned(),
    ];
    if let Some(model) = options.model.as_deref().filter(|m| !m.trim().is_empty()) {
        args.extend(["--model".into(), model.trim().into()]);
    }
    args
}

/// `claude -p --output-format json` prints one object with `result` and
/// `is_error`.
fn claude_answer(stdout: &str, stderr: &str) -> Result<String, AiError> {
    let p = Provider::Claude;
    let json = stdout
        .lines()
        .rev()
        .find_map(|l| {
            serde_json::from_str::<serde_json::Value>(l.trim())
                .ok()
                .filter(|v| v.is_object())
        })
        .or_else(|| serde_json::from_str(stdout.trim()).ok());
    let Some(json) = json else {
        return Err(failure(p, stdout, stderr));
    };
    let result = json
        .get("result")
        .and_then(|r| r.as_str())
        .unwrap_or_default()
        .trim()
        .to_owned();
    if json
        .get("is_error")
        .and_then(|e| e.as_bool())
        .unwrap_or(false)
        || result.is_empty()
    {
        let message = if result.is_empty() {
            json.get("subtype")
                .and_then(|s| s.as_str())
                .unwrap_or("no answer")
                .to_owned()
        } else {
            result
        };
        return Err(failure(p, &message, stderr));
    }
    Ok(result)
}

fn copilot_answer(success: bool, stdout: &str, stderr: &str) -> Result<String, AiError> {
    let text = stdout.trim();
    if !success || text.is_empty() || text.starts_with("Error:") {
        return Err(failure(Provider::Copilot, stdout, stderr));
    }
    Ok(text.to_owned())
}

/// Tells a sign-in problem apart from any other failure.
fn failure(provider: Provider, stdout: &str, stderr: &str) -> AiError {
    let message = [stdout.trim(), stderr.trim()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let lower = message.to_ascii_lowercase();
    let signed_out = [
        "/login",
        "not logged in",
        "log in",
        "login",
        "authenticat",
        "invalid api key",
        "unauthorized",
        "oauth",
    ]
    .iter()
    .any(|w| lower.contains(w));
    let message = if message.chars().count() > 600 {
        message.chars().take(600).collect::<String>() + "…"
    } else if message.is_empty() {
        "it exited without an answer".to_owned()
    } else {
        message
    };
    if signed_out {
        AiError::NotSignedIn(provider, message)
    } else {
        AiError::Failed(provider, message)
    }
}

/// A command for `path` that opens no console window on Windows.
fn command(path: &Path) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(path);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Looks for `name` on `PATH`, then in the folders installers put it in.
fn find_program(name: &str) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) {
        ["exe", "cmd", "bat"]
            .iter()
            .map(|ext| format!("{name}.{ext}"))
            .collect()
    } else {
        vec![name.to_owned()]
    };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    dirs.extend(install_dirs());
    dirs.iter()
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|candidate| candidate.is_file())
}

fn install_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    if let Some(home) = &home {
        for sub in [
            ".local/bin",
            ".claude/local",
            ".npm-global/bin",
            ".volta/bin",
            ".bun/bin",
            ".yarn/bin",
            "bin",
        ] {
            dirs.push(home.join(sub));
        }
        // nvm keeps one folder per Node version; the newest is likely the one in use.
        if let Ok(entries) = std::fs::read_dir(home.join(".nvm/versions/node")) {
            let mut versions: Vec<PathBuf> =
                entries.flatten().map(|e| e.path().join("bin")).collect();
            versions.sort();
            dirs.extend(versions.into_iter().rev());
        }
    }
    for var in ["APPDATA"] {
        if let Some(base) = std::env::var_os(var) {
            dirs.push(PathBuf::from(base).join("npm"));
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        dirs.push(local.join("Microsoft/WinGet/Links"));
        dirs.push(local.join("Programs/claude"));
    }
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(PathBuf::from));
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_claude_json() {
        let ok = r#"{"type":"result","subtype":"success","is_error":false,"result":" pong "}"#;
        assert_eq!(claude_answer(ok, "").unwrap(), "pong");
        let err = r#"{"type":"result","subtype":"success","is_error":true,"result":"Invalid API key · Please run /login"}"#;
        assert!(matches!(
            claude_answer(err, ""),
            Err(AiError::NotSignedIn(Provider::Claude, _))
        ));
        assert!(
            matches!(claude_answer("", "boom"), Err(AiError::Failed(Provider::Claude, m)) if m == "boom")
        );
    }

    #[test]
    fn reads_copilot_text() {
        assert_eq!(
            copilot_answer(true, "  The cause is X.\n", "").unwrap(),
            "The cause is X."
        );
        assert!(matches!(
            copilot_answer(false, "", "Error: No authentication information found."),
            Err(AiError::NotSignedIn(Provider::Copilot, _))
        ));
    }

    #[test]
    fn claude_runs_without_tools() {
        let args = claude_args(&AskOptions {
            model: Some("sonnet".into()),
            timeout_s: None,
        });
        let args: Vec<_> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[..5], ["-p", "--output-format", "json", "--tools", ""]);
        assert!(args.ends_with(&["--model".into(), "sonnet".into()]));
    }

    #[test]
    fn parses_provider_names() {
        assert_eq!("Claude".parse::<Provider>().unwrap(), Provider::Claude);
        assert_eq!("copilot".parse::<Provider>().unwrap(), Provider::Copilot);
        assert!("gpt".parse::<Provider>().is_err());
    }

    /// Runs a fake provider: a script that echoes stdin as Claude's JSON.
    #[cfg(unix)]
    #[tokio::test]
    async fn asks_a_program_and_reads_its_answer() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("claude");
        std::fs::write(
            &script,
            "#!/bin/sh\nread line\nprintf '{\"type\":\"result\",\"is_error\":false,\"result\":\"got %s\"}\\n' \"$line\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let found = Found {
            provider: Provider::Claude,
            path: script.to_string_lossy().into(),
            version: None,
        };
        let answer = ask(
            &found,
            "hello\n",
            &AskOptions::default(),
            std::future::pending(),
        )
        .await
        .unwrap();
        assert_eq!(answer.text, "got hello");

        // Cancelling kills a provider that never answers.
        std::fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
        let err = ask(&found, "x", &AskOptions::default(), async {})
            .await
            .unwrap_err();
        assert!(matches!(err, AiError::Cancelled));
    }
}
