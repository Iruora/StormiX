//! AI commands: explain a step or a load test with the user's own Claude
//! Code or GitHub Copilot CLI. See the `stormix-ai` crate.

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

/// Calls in flight, so Stop can end them.
#[derive(Default)]
pub struct AiCalls(Mutex<Vec<oneshot::Sender<()>>>);

/// The providers installed on this machine.
#[tauri::command]
#[specta::specta]
pub async fn ai_providers(paths: Vec<AiPath>) -> Vec<Found> {
    let paths: Vec<_> = paths.into_iter().map(|p| (p.provider, p.path)).collect();
    stormix_ai::detect_all(&paths).await
}

/// Builds the prompt for `question` with `secrets` and anything that looks
/// like a credential hidden, then asks the chosen provider unless `send` is
/// false (to show the user what would be sent).
#[tauri::command]
#[specta::specta]
pub async fn ai_explain(
    question: AiQuestion,
    secrets: Vec<String>,
    choice: AiChoice,
    send: bool,
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
    let found = find(&choice).await?;
    let (stop, stopped) = oneshot::channel();
    calls.0.lock().unwrap_or_else(|e| e.into_inner()).push(stop);
    let options = AskOptions {
        model: choice.model.clone(),
        timeout_s: None,
    };
    let answer = stormix_ai::ask(&found, &prompt, &options, async {
        // A dropped sender (the call list was cleared) also stops it.
        let _ = stopped.await;
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(AiReply {
        prompt,
        answer: Some(answer),
    })
}

/// Stops every AI call in flight.
#[tauri::command]
#[specta::specta]
pub fn ai_cancel(calls: State<'_, AiCalls>) {
    for stop in calls.0.lock().unwrap_or_else(|e| e.into_inner()).drain(..) {
        let _ = stop.send(());
    }
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
