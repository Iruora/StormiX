//! Responses kept for steps marked `cache: true`, so a run or a load test
//! iteration can reuse them instead of sending the request again.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

/// Which response a request gets back: a step's own, and only for the very
/// same request once variables are filled in. A new token in a header or a
/// new id in the URL is a different request, sent for real.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    flow: String,
    step: String,
    request: Vec<u8>,
}

impl Key {
    /// A streamed (multipart) body cannot be read back, so `extra` stands in
    /// for it: its fields as text.
    pub(crate) fn new(
        flow: &str,
        step: &str,
        request: &reqwest::Request,
        extra: Option<&str>,
    ) -> Self {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(request.method().as_str().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(request.url().as_str().as_bytes());
        for (name, value) in request.headers() {
            bytes.push(0);
            bytes.extend_from_slice(name.as_str().as_bytes());
            bytes.push(b':');
            bytes.extend_from_slice(value.as_bytes());
        }
        bytes.push(0);
        if let Some(body) = request.body().and_then(|b| b.as_bytes()) {
            bytes.extend_from_slice(body);
        }
        if let Some(extra) = extra {
            bytes.push(0);
            bytes.extend_from_slice(extra.as_bytes());
        }
        Self {
            flow: flow.to_owned(),
            step: step.to_owned(),
            request: bytes,
        }
    }
}

/// A response as it came back the first time.
#[derive(Debug)]
pub(crate) struct Stored {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: bytes::Bytes,
    /// Milliseconds since the Unix epoch.
    pub stored_at: u64,
}

/// What the cache holds for one step, for the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CachedStep {
    pub flow: String,
    pub step: String,
    /// Distinct requests with a response kept; usually one.
    pub responses: u32,
    /// When the newest was kept, in milliseconds since the Unix epoch.
    pub stored_at: f64,
}

/// Shared by every clone of a [`Runner`](crate::Runner), so a load test
/// fills and reads the same cache as the single runs before it. Kept in
/// memory only: it lasts as long as the runner.
#[derive(Debug, Default)]
pub struct ResponseCache {
    entries: Mutex<HashMap<Key, Arc<Stored>>>,
}

impl ResponseCache {
    fn lock(&self) -> MutexGuard<'_, HashMap<Key, Arc<Stored>>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn get(&self, key: &Key) -> Option<Arc<Stored>> {
        self.lock().get(key).cloned()
    }

    /// Keeps the first response; one already kept is not replaced.
    pub(crate) fn put(&self, key: Key, status: StatusCode, headers: HeaderMap, body: bytes::Bytes) {
        let stored_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        self.lock().entry(key).or_insert_with(|| {
            Arc::new(Stored {
                status,
                headers,
                body,
                stored_at,
            })
        });
    }

    /// Forgets the responses of one step, of every step of one flow, or
    /// (with neither) everything. Returns how many were forgotten.
    pub fn clear(&self, flow: Option<&str>, step: Option<&str>) -> usize {
        let mut entries = self.lock();
        let before = entries.len();
        entries.retain(|key, _| {
            let matches = flow.is_none_or(|f| key.flow == f) && step.is_none_or(|s| key.step == s);
            !matches
        });
        before - entries.len()
    }

    /// Each step with a response kept, by flow then step name.
    pub fn steps(&self) -> Vec<CachedStep> {
        let mut by_step: HashMap<(&str, &str), (u32, u64)> = HashMap::new();
        let entries = self.lock();
        for (key, stored) in entries.iter() {
            let entry = by_step.entry((&key.flow, &key.step)).or_default();
            entry.0 += 1;
            entry.1 = entry.1.max(stored.stored_at);
        }
        let mut steps: Vec<CachedStep> = by_step
            .into_iter()
            .map(|((flow, step), (responses, stored_at))| CachedStep {
                flow: flow.to_owned(),
                step: step.to_owned(),
                responses,
                stored_at: stored_at as f64,
            })
            .collect();
        steps.sort_by(|a, b| (&a.flow, &a.step).cmp(&(&b.flow, &b.step)));
        steps
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }
}
