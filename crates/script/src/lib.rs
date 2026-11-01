//! Runs the JavaScript that Postman and Bruno collections carry, in an
//! embedded QuickJS engine with no network or file access.
//!
//! Scripts see Postman's `pm` (and its legacy globals such as `tests` and
//! `responseBody`) and Bruno's `bru`, `req`, `res`, `test` and `expect`, so
//! one sandbox serves collections from either tool. Each run gets a fresh
//! engine, a memory cap and a time limit.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use rquickjs::{Context, Runtime};
use serde::{Deserialize, Serialize};

const PRELUDE: &str = include_str!("prelude.js");

/// Enough for scripts that parse sizeable JSON bodies; a runaway loop that
/// allocates hits it quickly.
const MEMORY_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    PreRequest,
    Test,
}

/// The request as a script sees and may change it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    /// Decoded as UTF-8, lossily.
    pub body: String,
    pub time_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptInput<'a> {
    pub phase: Phase,
    pub step_name: &'a str,
    pub variables: &'a BTreeMap<String, String>,
    pub request: &'a ScriptRequest,
    pub response: Option<&'a ScriptResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TestOutcome {
    pub name: String,
    pub passed: bool,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ScriptOutput {
    /// Variables the script set (`Some`) or removed (`None`), in name order.
    pub variables: BTreeMap<String, Option<String>>,
    /// `pm.test`, `test` and legacy `tests[...]` results, in the order they ran.
    pub tests: Vec<TestOutcome>,
    /// What the script passed to `console.*`.
    pub logs: Vec<String>,
    /// The request after the script's changes (pre-request scripts only matter).
    pub request: ScriptRequest,
    /// The exception that stopped the script, if one did. Tests that ran
    /// before it still count.
    pub error: Option<String>,
}

/// Sends an HTTP request for `bru.sendRequest` / `pm.sendRequest`. It gets
/// the time the script has left, and should not take longer.
pub type Sender<'a> =
    &'a (dyn Fn(&ScriptRequest, Duration) -> Result<ScriptResponse, String> + Sync);

/// Runs `source`. Never panics on bad scripts: syntax errors, exceptions,
/// timeouts and running out of memory all come back as `error`. Scripts
/// cannot send requests; see [`run_with`].
pub fn run(source: &str, input: &ScriptInput<'_>, timeout: Duration) -> ScriptOutput {
    run_with(source, input, timeout, None)
}

/// Like [`run`], with `send` doing the requests a script asks for.
pub fn run_with(
    source: &str,
    input: &ScriptInput<'_>,
    timeout: Duration,
    send: Option<Sender<'_>>,
) -> ScriptOutput {
    match run_inner(source, input, timeout, send) {
        Ok(output) => output,
        Err(error) => ScriptOutput {
            variables: BTreeMap::new(),
            tests: Vec::new(),
            logs: Vec::new(),
            request: input.request.clone(),
            error: Some(error),
        },
    }
}

fn run_inner(
    source: &str,
    input: &ScriptInput<'_>,
    timeout: Duration,
    send: Option<Sender<'_>>,
) -> Result<ScriptOutput, String> {
    let runtime = Runtime::new().map_err(|e| format!("cannot start the script engine: {e}"))?;
    runtime.set_memory_limit(MEMORY_LIMIT);
    let deadline = Instant::now() + timeout;
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline)));
    let context =
        Context::full(&runtime).map_err(|e| format!("cannot start the script engine: {e}"))?;

    let input_json = serde_json::to_string(input).expect("script input always serializes");
    let started = context.with(|ctx| -> rquickjs::Result<()> {
        ctx.globals().set("__INPUT", input_json)?;
        // `bru.sleep` blocks this thread; scripts already run off the async
        // runtime. It never sleeps past the deadline.
        let sleep = rquickjs::Function::new(ctx.clone(), move |ms: f64| {
            let left = deadline.saturating_duration_since(Instant::now());
            std::thread::sleep(Duration::from_secs_f64(ms / 1000.0).min(left));
        })?;
        ctx.globals().set("__sleep", sleep)?;
        ctx.eval::<(), _>(PRELUDE)?;
        let run: rquickjs::Function = ctx.globals().get("__run")?;
        run.call((source,))
    });
    // Let `await` in the script resume until nothing is left to do. A job
    // that throws only ends up rejecting the script's promise, which
    // `__run` already records, so job errors need no handling here. Requests
    // the script sends wait in an outbox: they are sent here, outside the
    // engine, and their replies delivered before resuming.
    let result = started.and_then(|()| loop {
        while runtime.is_job_pending() && Instant::now() <= deadline {
            let _ = runtime.execute_pending_job();
        }
        if Instant::now() > deadline {
            return Err(rquickjs::Error::Exception);
        }
        let outbox: String = context.with(|ctx| {
            let take: rquickjs::Function = ctx.globals().get("__takeOutbox")?;
            take.call(())
        })?;
        let calls: Vec<Outgoing> = serde_json::from_str(&outbox).unwrap_or_default();
        if calls.is_empty() {
            break context.with(|ctx| -> rquickjs::Result<String> {
                let finish: rquickjs::Function = ctx.globals().get("__finish")?;
                finish.call(())
            });
        }
        for call in calls {
            let left = deadline.saturating_duration_since(Instant::now());
            let reply = match send {
                Some(send) => send(&call.request, left),
                None => Err("sending requests from scripts is not available here".into()),
            };
            let reply = match reply {
                Ok(response) => serde_json::json!({ "response": response }),
                Err(error) => serde_json::json!({ "error": error }),
            };
            context.with(|ctx| -> rquickjs::Result<()> {
                let deliver: rquickjs::Function = ctx.globals().get("__deliver")?;
                deliver.call((call.id, reply.to_string()))
            })?;
        }
    });
    match result {
        Ok(json) => {
            serde_json::from_str(&json).map_err(|e| format!("unexpected script result: {e}"))
        }
        Err(_) if Instant::now() > deadline => Err(format!(
            "the script did not finish within {} ms",
            timeout.as_millis()
        )),
        Err(rquickjs::Error::Allocation) => Err("the script ran out of memory".into()),
        Err(e) => Err(format!("the script engine failed: {e}")),
    }
}

/// A request a script is waiting on.
#[derive(Deserialize)]
struct Outgoing {
    id: u32,
    request: ScriptRequest,
}

#[cfg(test)]
mod tests;
