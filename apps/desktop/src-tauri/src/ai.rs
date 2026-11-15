//! AI commands: explain a step or a load test with the user's own Claude
//! Code or GitHub Copilot CLI. See the `stormix-ai` crate.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use stormix_ai::{Answer, AskOptions, Found, Provider, Redactor};
use stormix_engine::{Diagnosis, LoadReport, StepResult};
use tauri::State;
use tokio::sync::oneshot;

/// A program path the user set by hand, for a provider not on `PATH`.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiPath {
    pub provider: Provider,
    pub path: String,
}

/// Which provider to ask, and how.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiChoice {
    /// `None` asks the first provider found.
    pub provider: Option<Provider>,
    #[serde(default)]
    pub paths: Vec<AiPath>,
    pub model: Option<String>,
}

/// What to explain.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AiQuestion {
    /// One step of a run or a load test sample.
    #[serde(rename_all = "camelCase")]
    Step {
        step: Box<StepResult>,
        /// The test file the step came from.
        file: Option<String>,
        diagnosis: Option<Box<Diagnosis>>,
    },
    #[serde(rename_all = "camelCase")]
    Load {
        report: Box<LoadReport>,
        file: Option<String>,
    },
}

/// The prompt as sent, with secrets hidden, and the answer when one was asked for.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiReply {
    pub prompt: String,
    pub answer: Option<Answer>,
}

/// Calls in flight by the id the panel gave them, so Stop and closing a
/// panel end only that panel's call.
#[derive(Default)]
pub struct AiCalls(Mutex<HashMap<String, Call>>);

enum Call {
    Running(oneshot::Sender<()>),
    /// Stopped before it was registered: it must not start.
    Stopped,
}

impl AiCalls {
    /// Registers call `id`, unless it was already stopped.
    fn start(&self, id: &str, stop: oneshot::Sender<()>) -> bool {
        let mut calls = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(calls.remove(id), Some(Call::Stopped)) {
            return false;
        }
        calls.insert(id.to_owned(), Call::Running(stop));
        true
    }

    fn finish(&self, id: &str) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
    }

    /// Stops call `id`, or marks it so it never starts if it isn't
    /// registered yet.
    fn stop(&self, id: &str) {
        if id.is_empty() {
            return;
        }
        let mut calls = self.0.lock().unwrap_or_else(|e| e.into_inner());
        match calls.insert(id.to_owned(), Call::Stopped) {
            Some(Call::Running(stop)) => {
                calls.remove(id);
                let _ = stop.send(());
            }
            Some(Call::Stopped) | None => {}
        }
    }
}

/// The providers installed on this machine.
#[tauri::command]
#[specta::specta]
pub async fn ai_providers(paths: Vec<AiPath>) -> Vec<Found> {
    let paths: Vec<_> = paths.into_iter().map(|p| (p.provider, p.path)).collect();
    stormix_ai::detect_all(&paths).await
}

/// Builds the prompt for `question` with `secrets` and anything that looks
/// like a credential hidden, then asks the chosen provider unless `send` is
/// false (to show the user what would be sent). `id` names the call for
/// [`ai_cancel`].
#[tauri::command]
#[specta::specta]
pub async fn ai_explain(
    question: AiQuestion,
    secrets: Vec<String>,
    choice: AiChoice,
    send: bool,
    id: String,
    calls: State<'_, AiCalls>,
) -> Result<AiReply, String> {
    let redactor = Redactor::new(secrets);
    let prompt = match &question {
        AiQuestion::Step {
            step,
            file,
            diagnosis,
        } => {
            stormix_ai::prompt::explain_step(step, file.as_deref(), diagnosis.as_deref(), &redactor)
        }
        AiQuestion::Load { report, file } => {
            stormix_ai::prompt::explain_load(report, file.as_deref(), &redactor)
        }
    };
    if !send {
        return Ok(AiReply {
            prompt,
            answer: None,
        });
    }
    // Registered before looking for the program, so a Stop during that
    // search counts too.
    let (stop, mut stopped) = oneshot::channel();
    if !calls.start(&id, stop) {
        return Err(stormix_ai::AiError::Cancelled.to_string());
    }
    let result = async {
        let found = tokio::select! {
            // A Stop wins when both are ready.
            biased;
            _ = &mut stopped => return Err(stormix_ai::AiError::Cancelled.to_string()),
            found = find(&choice) => found?,
        };
        let options = AskOptions {
            model: choice.model.clone(),
            timeout_s: None,
        };
        stormix_ai::ask(&found, &prompt, &options, async {
            // A dropped sender also stops it.
            let _ = stopped.await;
        })
        .await
        .map_err(|e| e.to_string())
    }
    .await;
    calls.finish(&id);
    let answer = result?;
    Ok(AiReply {
        prompt,
        answer: Some(answer),
    })
}

/// Stops the AI call `id`, also one asked for but not started yet.
#[tauri::command]
#[specta::specta]
pub fn ai_cancel(id: String, calls: State<'_, AiCalls>) {
    calls.stop(&id);
}

async fn find(choice: &AiChoice) -> Result<Found, String> {
    let path_of = |p: Provider| {
        choice
            .paths
            .iter()
            .find(|x| x.provider == p && !x.path.trim().is_empty())
            .map(|x| std::path::PathBuf::from(x.path.trim()))
    };
    match choice.provider {
        Some(p) => stormix_ai::detect(p, path_of(p).as_deref())
            .await
            .ok_or_else(|| stormix_ai::AiError::NotFound(p).to_string()),
        None => {
            let paths: Vec<_> = choice
                .paths
                .iter()
                .map(|p| (p.provider, p.path.clone()))
                .collect();
            stormix_ai::detect_all(&paths).await.into_iter().next().ok_or_else(|| {
                "No AI command line was found. Install Claude Code or GitHub Copilot CLI and sign in, \
                 or set its path in Settings."
                    .to_owned()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stop_before_the_start_wins() {
        let calls = AiCalls::default();
        calls.stop("a");
        let (tx, _rx) = oneshot::channel();
        assert!(!calls.start("a", tx));
        // The mark is used up: the same id may run again later.
        let (tx, mut rx) = oneshot::channel();
        assert!(calls.start("a", tx));
        calls.stop("a");
        assert!(rx.try_recv().is_ok());
        assert!(calls.0.lock().unwrap().is_empty());
    }
}
