//! Reads the API's own CPU and memory use while a load test runs, from a
//! metrics endpoint it serves: Prometheus text (Spring Boot Actuator with
//! Micrometer, prometheus-net, prom-client, client_golang, Quarkus...) or
//! Spring Boot Actuator's JSON `/metrics`.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use stormix_flow::{Monitor, MonitorTarget};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::Variables;

/// What one endpoint reported over a load test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ResourceSeries {
    /// The monitor's `name`, else the URL's host.
    pub name: String,
    /// The URL read, without its query string or any password.
    pub url: String,
    /// How the figures were read: `prometheus`, or `actuator` for
    /// Actuator's JSON `/metrics`.
    pub format: String,
    /// CPU cores the API's machine or container has, when it says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<f64>,
    /// JVM only: the most heap the API may use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heap_max_bytes: Option<f64>,
    /// What `memoryBytes` measures: `resident` (the process's resident
    /// memory, RSS) or `jvm` (JVM heap and non-heap in use).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_kind: Option<String>,
    /// One per read, oldest first. The first is read before the load starts.
    pub samples: Vec<ResourceSample>,
    /// Reads that failed.
    pub failed_reads: u32,
    /// Why the latest failed read failed, like `404 Not Found`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The health check read beside the metrics, shown without its query
    /// string or password. None without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_url: Option<String>,
    /// The API's health over the test: its first reading, then each change.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub health: Vec<HealthChange>,
    /// Why the health check could not be read at all (left alone after
    /// that), like `404 Not Found` when it is not exposed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_error: Option<String>,
    /// Whether the latest read failed (or none can be made), for live
    /// views; `error` keeps the last failure's reason after that.
    #[serde(skip)]
    pub failing: bool,
}

/// The API's health from a moment on, until the next change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HealthChange {
    /// Seconds since the load started.
    pub t: f64,
    /// `UP`, `DOWN`, `OUT_OF_SERVICE` or `UNKNOWN` as the API reports it;
    /// `UNREACHABLE` when it did not answer; `HTTP 503` for an error
    /// status without a JSON `status`.
    pub status: String,
    /// What is not up, like `db: DOWN, diskSpace: DOWN`, or why it did not
    /// answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl HealthChange {
    /// `UP` and `UNKNOWN` are not counted against the API.
    pub fn unhealthy(&self) -> bool {
        !matches!(self.status.as_str(), "UP" | "UNKNOWN")
    }
}

/// A stretch of time the API was not healthy.
#[derive(Debug, Clone, PartialEq)]
pub struct UnhealthyPeriod {
    pub from: f64,
    /// None while it lasted until the end.
    pub to: Option<f64>,
    pub status: String,
    pub detail: Option<String>,
}

/// The API's resource use at one moment.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ResourceSample {
    /// Seconds since the load started (0 for the read just before it).
    pub t: f64,
    /// CPU in use, where 100 is one full core (as `top` shows it). Empty on
    /// the first read when only a CPU-time counter is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<f64>,
    /// See [`ResourceSeries::memory_kind`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<f64>,
    /// JVM only: heap in use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heap_bytes: Option<f64>,
    /// Garbage collection pauses, in milliseconds per second (JVM, Node).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gc_ms_per_s: Option<f64>,
}

/// A series summed up, for reports.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResourceStats {
    pub cpu_avg: Option<f64>,
    pub cpu_peak: Option<f64>,
    /// The first reading, before the load.
    pub memory_start: Option<f64>,
    pub memory_peak: Option<f64>,
    pub memory_end: Option<f64>,
    pub heap_peak: Option<f64>,
    pub gc_avg: Option<f64>,
}

impl ResourceSeries {
    /// Each time the API was unhealthy, in order.
    pub fn unhealthy_periods(&self) -> Vec<UnhealthyPeriod> {
        let mut periods: Vec<UnhealthyPeriod> = Vec::new();
        for (i, change) in self.health.iter().enumerate() {
            if !change.unhealthy() {
                continue;
            }
            // A change from one unhealthy state to another continues it.
            if i > 0 && self.health[i - 1].unhealthy() {
                continue;
            }
            let to = self.health[i + 1..]
                .iter()
                .find(|c| !c.unhealthy())
                .map(|c| c.t);
            periods.push(UnhealthyPeriod {
                from: change.t,
                to,
                status: change.status.clone(),
                detail: change.detail.clone(),
            });
        }
        periods
    }

    pub fn stats(&self) -> ResourceStats {
        let values = |f: fn(&ResourceSample) -> Option<f64>| -> Vec<f64> {
            self.samples.iter().filter_map(f).collect()
        };
        let avg = |v: &[f64]| (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64);
        let peak = |v: &[f64]| v.iter().copied().reduce(f64::max);
        let cpu = values(|s| s.cpu_percent);
        let memory = values(|s| s.memory_bytes);
        let gc = values(|s| s.gc_ms_per_s);
        ResourceStats {
            cpu_avg: avg(&cpu),
            cpu_peak: peak(&cpu),
            memory_start: memory.first().copied(),
            memory_peak: peak(&memory),
            memory_end: memory.last().copied(),
            heap_peak: peak(&values(|s| s.heap_bytes)),
            gc_avg: avg(&gc),
        }
    }
}

/// A read's news for the series it belongs to.
#[derive(Debug)]
pub(crate) struct Update {
    pub index: usize,
    pub outcome: Result<Read, String>,
    pub health: Option<HealthNews>,
}

#[derive(Debug)]
pub(crate) enum HealthNews {
    Reading(HealthChange),
    /// The check cannot be read at all, so it is no longer tried.
    Unavailable(String),
}

#[derive(Debug)]
pub(crate) struct Read {
    pub sample: ResourceSample,
    pub format: &'static str,
    pub cpu_cores: Option<f64>,
    pub heap_max_bytes: Option<f64>,
    pub memory_kind: Option<&'static str>,
}

impl ResourceSeries {
    pub(crate) fn apply(&mut self, update: Update) {
        match update.health {
            Some(HealthNews::Reading(change)) => {
                let same = self
                    .health
                    .last()
                    .is_some_and(|l| l.status == change.status && l.detail == change.detail);
                if !same {
                    self.health.push(change);
                }
            }
            Some(HealthNews::Unavailable(error)) => self.health_error = Some(error),
            None => {}
        }
        match update.outcome {
            Ok(read) => {
                self.format = read.format.to_owned();
                self.cpu_cores = read.cpu_cores.or(self.cpu_cores);
                self.heap_max_bytes = read.heap_max_bytes.or(self.heap_max_bytes);
                if let Some(kind) = read.memory_kind {
                    self.memory_kind = Some(kind.to_owned());
                }
                self.samples.push(read.sample);
                self.failing = false;
            }
            Err(error) => {
                self.failed_reads += 1;
                self.error = Some(error);
                self.failing = true;
            }
        }
    }
}

/// Reads every monitor until dropped. Returns the empty series, in order,
/// and the tasks filling them through `tx`.
pub(crate) fn start(
    monitors: &[Monitor],
    vars: &Variables,
    client: &reqwest::Client,
    origin: Instant,
    tx: &mpsc::UnboundedSender<Update>,
) -> (Vec<ResourceSeries>, JoinSet<()>) {
    let mut tasks = JoinSet::new();
    let series = monitors
        .iter()
        .enumerate()
        .map(|(index, monitor)| {
            let target = monitor.target();
            let mut missing = Vec::new();
            let mut fill = |text: &str| {
                let rendered = vars.render(text);
                missing.extend(rendered.missing);
                rendered.text
            };
            let url = fill(&target.url);
            let headers: Vec<(String, String)> = target
                .headers
                .iter()
                .map(|h| (h.name.clone(), fill(&h.value)))
                .collect();
            let parsed = reqwest::Url::parse(url.trim());
            let mut series = ResourceSeries {
                // A URL that does not parse is never copied into the
                // report: it may hold a token that `shown` would remove.
                name: target.name.clone().unwrap_or_else(|| {
                    parsed
                        .as_ref()
                        .ok()
                        .and_then(|u| u.host_str().map(str::to_owned))
                        .unwrap_or_else(|| format!("monitor {}", index + 1))
                }),
                url: parsed
                    .as_ref()
                    .map_or_else(|_| "(not a valid URL)".to_owned(), shown),
                format: monitor.kind().to_owned(),
                cpu_cores: None,
                heap_max_bytes: None,
                memory_kind: None,
                samples: Vec::new(),
                failed_reads: 0,
                error: None,
                health_url: None,
                health: Vec::new(),
                health_error: None,
                failing: false,
            };
            let health = monitor.health_url().map(|h| fill(&h));
            if let Some(name) = missing.first() {
                series.error = Some(format!("`{{{{{name}}}}}` has no value"));
                series.failing = true;
            } else {
                match parsed {
                    Ok(url) => {
                        let health = match health.map(|h| reqwest::Url::parse(h.trim())) {
                            Some(Ok(h)) => {
                                series.health_url = Some(shown(&h));
                                Some(h)
                            }
                            Some(Err(e)) => {
                                series.health_error =
                                    Some(format!("the health URL is not valid: {e}"));
                                None
                            }
                            None => None,
                        };
                        let reader = Reader {
                            client: client.clone(),
                            url,
                            headers,
                            actuator: matches!(monitor, Monitor::Actuator(_)),
                            timeout: timeout(target),
                            health,
                        };
                        tasks.spawn(watch(index, reader, target.every(), origin, tx.clone()));
                    }
                    Err(e) => {
                        series.error = Some(format!("the URL is not valid: {e}"));
                        series.failing = true;
                    }
                }
            }
            series
        })
        .collect();
    (series, tasks)
}

fn timeout(target: &MonitorTarget) -> Duration {
    target
        .every()
        .max(Duration::from_secs(2))
        .min(Duration::from_secs(10))
}

/// The URL as reports show it: no password, no query (it may hold a token).
fn shown(url: &reqwest::Url) -> String {
    let mut url = url.clone();
    let _ = url.set_password(None);
    url.set_query(None);
    url.to_string()
}

async fn watch(
    index: usize,
    reader: Reader,
    every: Duration,
    origin: Instant,
    tx: mpsc::UnboundedSender<Update>,
) {
    let mut tick = tokio::time::interval(every);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut mode = None;
    let mut previous: Option<(Instant, Counters)> = None;
    let mut health_on = reader.health.is_some();
    let mut health_seen = false;
    loop {
        tick.tick().await;
        let at = Instant::now();
        let elapsed = at.saturating_duration_since(origin).as_secs_f64();
        let (metrics, health) = tokio::join!(reader.read(&mut mode), async {
            if health_on {
                reader.health().await
            } else {
                None
            }
        });
        let health = health.map(|outcome| match outcome {
            // A check that is not there or not allowed from the start is
            // left alone, rather than counting the API as unhealthy.
            HealthOutcome::Refused(error) if !health_seen => {
                health_on = false;
                HealthNews::Unavailable(error)
            }
            other => {
                health_seen = true;
                HealthNews::Reading(other.into_change((elapsed * 10.0).round() / 10.0))
            }
        });
        let outcome = match metrics {
            Ok(figures) => {
                let since = previous
                    .as_ref()
                    .map(|(then, counters)| (at.duration_since(*then).as_secs_f64(), counters));
                let read = figures.to_read(elapsed, since, mode.unwrap_or(Mode::Prometheus));
                previous = Some((at, figures.counters));
                Ok(read)
            }
            Err(e) => Err(e),
        };
        if tx
            .send(Update {
                index,
                outcome,
                health,
            })
            .is_err()
        {
            return;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Prometheus,
    /// Actuator without the Prometheus registry: one JSON call per metric.
    ActuatorJson,
}

struct Reader {
    client: reqwest::Client,
    url: reqwest::Url,
    headers: Vec<(String, String)>,
    actuator: bool,
    timeout: Duration,
    health: Option<reqwest::Url>,
}

/// What one read of the health check found.
#[derive(Debug, PartialEq)]
enum HealthOutcome {
    /// It answered, with a status (`UP`, `DOWN`, `HTTP 503`...).
    Status(String, Option<String>),
    /// 401, 403 or 404: not exposed, or not for us.
    Refused(String),
    Unreachable(String),
}

impl HealthOutcome {
    fn into_change(self, t: f64) -> HealthChange {
        let (status, detail) = match self {
            Self::Status(status, detail) => (status, detail),
            Self::Refused(error) => (
                error
                    .split(' ')
                    .next()
                    .map_or(error.clone(), |code| format!("HTTP {code}")),
                Some(error),
            ),
            Self::Unreachable(error) => ("UNREACHABLE".to_owned(), Some(error)),
        };
        HealthChange { t, status, detail }
    }

    /// From a health check's status code and body: Spring Boot's
    /// `{"status":"DOWN","components":{...}}`, or any other answer.
    fn from_response(code: reqwest::StatusCode, body: &str) -> Self {
        let json: Option<serde_json::Value> = serde_json::from_str(body).ok();
        let reported = json
            .as_ref()
            .and_then(|j| j.get("status"))
            .and_then(|s| s.as_str())
            .map(str::to_uppercase);
        let status = match reported {
            Some(status) => status,
            None if code.is_success() => "UP".to_owned(),
            None => format!("HTTP {}", code.as_u16()),
        };
        let mut down = Vec::new();
        if let Some(json) = &json {
            not_up(json, "", &mut down);
        }
        let detail = if down.is_empty() {
            (status != "UP" && json.is_none() && !code.is_success()).then(|| code.to_string())
        } else {
            Some(down.join(", "))
        };
        Self::Status(status, detail)
    }
}

/// The components of a Spring Boot health answer that are not `UP`, as
/// `db: DOWN` or `db.primary: DOWN` for nested ones, deepest only.
fn not_up(json: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
    // Spring Boot 2.2+ says `components`, older versions `details`.
    let Some(components) = json
        .get("components")
        .or_else(|| json.get("details"))
        .and_then(|c| c.as_object())
    else {
        return;
    };
    for (name, component) in components {
        let Some(status) = component.get("status").and_then(|s| s.as_str()) else {
            continue;
        };
        if status.eq_ignore_ascii_case("UP") {
            continue;
        }
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };
        let before = out.len();
        not_up(component, &path, out);
        if out.len() == before {
            out.push(format!("{path}: {status}"));
        }
    }
}

impl Reader {
    /// One reading. For Actuator, the first success decides between its
    /// `/prometheus` and its JSON `/metrics`.
    async fn read(&self, mode: &mut Option<Mode>) -> Result<Figures, String> {
        if !self.actuator {
            *mode = Some(Mode::Prometheus);
            return self.prometheus(self.url.clone()).await;
        }
        let base = self.url.as_str().trim_end_matches('/').to_owned();
        let prometheus = format!("{base}/prometheus");
        match mode {
            Some(Mode::Prometheus) => self.prometheus(parse(&prometheus)?).await,
            Some(Mode::ActuatorJson) => self.actuator_json(&base).await,
            None => match self.prometheus(parse(&prometheus)?).await {
                Ok(figures) if figures.found_any() => {
                    *mode = Some(Mode::Prometheus);
                    Ok(figures)
                }
                first => match self.actuator_json(&base).await {
                    Ok(figures) => {
                        *mode = Some(Mode::ActuatorJson);
                        Ok(figures)
                    }
                    Err(json) => Err(match first {
                        Err(e) => format!("{e} (and /metrics: {json})"),
                        Ok(_) => json,
                    }),
                },
            },
        }
    }

    /// One read of the health check, when there is one.
    async fn health(&self) -> Option<HealthOutcome> {
        let url = self.health.clone()?;
        let mut request = self
            .client
            .get(url)
            .timeout(self.timeout)
            .header(reqwest::header::ACCEPT, "application/json");
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }
        Some(match request.send().await {
            // The deepest cause says it best: `Connection refused (os error 111)`.
            Err(e) => HealthOutcome::Unreachable(
                describe(&e.without_url())
                    .rsplit(": ")
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
            ),
            Ok(response) => {
                let code = response.status();
                if matches!(code.as_u16(), 401 | 403 | 404) {
                    HealthOutcome::Refused(code.to_string())
                } else {
                    let body = response.text().await.unwrap_or_default();
                    HealthOutcome::from_response(code, &body)
                }
            }
        })
    }

    async fn get(&self, url: reqwest::Url, accept: &str) -> Result<String, String> {
        let mut request = self
            .client
            .get(url)
            .timeout(self.timeout)
            .header(reqwest::header::ACCEPT, accept);
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }
        let response = request
            .send()
            .await
            .map_err(|e| describe(&e.without_url()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(status.to_string());
        }
        response
            .text()
            .await
            .map_err(|e| describe(&e.without_url()))
    }

    async fn prometheus(&self, url: reqwest::Url) -> Result<Figures, String> {
        let text = self
            .get(url, "text/plain;version=0.0.4;q=1,*/*;q=0.1")
            .await?;
        let figures = Figures::from_prometheus(&text);
        if figures.found_any() {
            Ok(figures)
        } else {
            Err("no CPU or memory metrics in the response".into())
        }
    }

    /// Actuator's `/metrics/{name}` JSON, one call per metric, together.
    async fn actuator_json(&self, base: &str) -> Result<Figures, String> {
        let get = |path: &'static str| {
            let url = format!("{base}/metrics/{path}");
            let statistic = if path.starts_with("jvm.gc") {
                "TOTAL_TIME"
            } else {
                "VALUE"
            };
            async move {
                let text = self.get(parse(&url)?, "application/json").await?;
                actuator_value(&text, statistic)
            }
        };
        let (cpu, cores, heap, nonheap, heap_max, gc) = tokio::join!(
            get("process.cpu.usage"),
            get("system.cpu.count"),
            get("jvm.memory.used?tag=area:heap"),
            get("jvm.memory.used?tag=area:nonheap"),
            get("jvm.memory.max?tag=area:heap"),
            get("jvm.gc.pause"),
        );
        // The CPU figure must be there: it says this is Actuator at all.
        let cpu = cpu?;
        let heap = heap.ok();
        Ok(Figures {
            cpu_usage: Some(cpu),
            cpu_count: cores.ok(),
            heap_used: heap,
            jvm_used: heap.zip(nonheap.ok()).map(|(h, n)| h + n).or(heap),
            heap_max: heap_max.ok().filter(|m| *m > 0.0),
            counters: Counters {
                gc_seconds: gc.ok(),
                ..Counters::default()
            },
            ..Figures::default()
        })
    }
}

fn parse(url: &str) -> Result<reqwest::Url, String> {
    reqwest::Url::parse(url).map_err(|e| format!("the URL is not valid: {e}"))
}

fn describe(err: &reqwest::Error) -> String {
    use std::error::Error as _;
    if err.is_timeout() {
        return "timed out".into();
    }
    let mut message = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

/// A measurement from Actuator's `{"measurements":[{"statistic":..,"value":..}]}`.
fn actuator_value(text: &str, statistic: &str) -> Result<f64, String> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "the response is not JSON".to_owned())?;
    json.get("measurements")
        .and_then(|m| m.as_array())
        .and_then(|list| {
            list.iter()
                .find(|m| m.get("statistic").and_then(|s| s.as_str()) == Some(statistic))
                .or_else(|| list.first())
        })
        .and_then(|m| m.get("value"))
        .and_then(serde_json::Value::as_f64)
        .ok_or_else(|| "the response has no measurements".into())
}

/// Counters, whose rate between two reads is the figure.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Counters {
    cpu_seconds: Option<f64>,
    gc_seconds: Option<f64>,
}

/// What one read of the endpoint found.
#[derive(Debug, Clone, Default, PartialEq)]
struct Figures {
    /// Micrometer's `process_cpu_usage`: 0 to 1 of the whole machine.
    cpu_usage: Option<f64>,
    cpu_count: Option<f64>,
    resident: Option<f64>,
    heap_used: Option<f64>,
    /// Heap and non-heap.
    jvm_used: Option<f64>,
    heap_max: Option<f64>,
    counters: Counters,
}

impl Figures {
    fn found_any(&self) -> bool {
        self.cpu_usage.is_some()
            || self.counters.cpu_seconds.is_some()
            || self.resident.is_some()
            || self.jvm_used.is_some()
    }

    fn from_prometheus(text: &str) -> Self {
        let mut f = Self::default();
        let mut heap = None::<f64>;
        let mut nonheap = None::<f64>;
        let mut heap_max = None::<f64>;
        let mut gc_jvm = None::<f64>;
        let mut gc_node = None::<f64>;
        let add = |slot: &mut Option<f64>, v: f64| *slot = Some(slot.unwrap_or(0.0) + v);
        for metric in parse_prometheus(text) {
            let v = metric.value;
            if !v.is_finite() {
                continue;
            }
            match metric.name {
                "process_cpu_seconds_total" => f.counters.cpu_seconds = Some(v),
                // Micrometer 1.13+, when there is no seconds counter.
                "process_cpu_time_ns_total" if f.counters.cpu_seconds.is_none() => {
                    f.counters.cpu_seconds = Some(v / 1e9);
                }
                "process_cpu_usage" => f.cpu_usage = Some(v),
                "system_cpu_count" => f.cpu_count = Some(v),
                "process_resident_memory_bytes" => f.resident = Some(v),
                "jvm_memory_used_bytes" => match metric.label("area") {
                    Some("heap") => add(&mut heap, v),
                    Some("nonheap") => add(&mut nonheap, v),
                    _ => {}
                },
                "jvm_memory_max_bytes" if metric.label("area") == Some("heap") && v > 0.0 => {
                    add(&mut heap_max, v);
                }
                "jvm_gc_pause_seconds_sum" => add(&mut gc_jvm, v),
                "nodejs_gc_duration_seconds_sum" => add(&mut gc_node, v),
                _ => {}
            }
        }
        f.heap_used = heap;
        f.jvm_used = match (heap, nonheap) {
            (Some(h), Some(n)) => Some(h + n),
            (h, n) => h.or(n),
        };
        f.heap_max = heap_max;
        f.counters.gc_seconds = gc_jvm.or(gc_node);
        f
    }

    /// The sample for this read, `since` being the time since the previous
    /// read and that read's counters.
    fn to_read(&self, t: f64, since: Option<(f64, &Counters)>, mode: Mode) -> Read {
        let rate = |now: Option<f64>, before: fn(&Counters) -> Option<f64>| {
            let (secs, counters) = since?;
            let delta = now? - before(counters)?;
            // A counter going down means the API restarted.
            (secs > 0.0 && delta >= 0.0).then(|| delta / secs)
        };
        // A CPU-time counter is exact between two reads, so when there is
        // one the first read has no CPU figure rather than Micrometer's
        // gauge, which can be far off just after the API starts.
        let cpu_percent = if self.counters.cpu_seconds.is_some() {
            rate(self.counters.cpu_seconds, |c| c.cpu_seconds).map(|cores| cores * 100.0)
        } else {
            // Micrometer's figure is of the whole machine.
            self.cpu_usage
                .map(|usage| usage * self.cpu_count.unwrap_or(1.0) * 100.0)
        };
        let (memory_bytes, memory_kind) = match (self.resident, self.jvm_used) {
            (Some(rss), _) => (Some(rss), Some("resident")),
            (None, Some(jvm)) => (Some(jvm), Some("jvm")),
            (None, None) => (None, None),
        };
        Read {
            sample: ResourceSample {
                t: (t * 10.0).round() / 10.0,
                cpu_percent: cpu_percent.map(|c| (c * 10.0).round() / 10.0),
                memory_bytes,
                heap_bytes: self.heap_used,
                gc_ms_per_s: rate(self.counters.gc_seconds, |c| c.gc_seconds)
                    .map(|s| (s * 10_000.0).round() / 10.0),
            },
            format: match mode {
                Mode::Prometheus => "prometheus",
                Mode::ActuatorJson => "actuator",
            },
            cpu_cores: self.cpu_count,
            heap_max_bytes: self.heap_max,
            memory_kind,
        }
    }
}

/// One line of the Prometheus text format.
#[derive(Debug, PartialEq)]
struct Metric<'a> {
    name: &'a str,
    labels: Vec<(&'a str, String)>,
    value: f64,
}

impl Metric<'_> {
    fn label(&self, name: &str) -> Option<&str> {
        self.labels
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// The samples of a Prometheus (or OpenMetrics) text page. Lines that do
/// not parse are skipped.
fn parse_prometheus(text: &str) -> impl Iterator<Item = Metric<'_>> {
    text.lines().filter_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let name_end = line.find(|c: char| c == '{' || c.is_whitespace())?;
        let name = &line[..name_end];
        let mut rest = &line[name_end..];
        let mut labels = Vec::new();
        if rest.starts_with('{') {
            let (parsed, after) = parse_labels(&rest[1..])?;
            labels = parsed;
            rest = after;
        }
        let value = rest.split_whitespace().next()?;
        let value = match value {
            "+Inf" | "Inf" => f64::INFINITY,
            "-Inf" => f64::NEG_INFINITY,
            "NaN" => f64::NAN,
            v => v.parse().ok()?,
        };
        Some(Metric {
            name,
            labels,
            value,
        })
    })
}

/// `key="value",...}` and what follows the closing brace.
#[allow(clippy::type_complexity)]
fn parse_labels(mut s: &str) -> Option<(Vec<(&str, String)>, &str)> {
    let mut labels = Vec::new();
    loop {
        s = s.trim_start_matches([',', ' ']);
        if let Some(after) = s.strip_prefix('}') {
            return Some((labels, after));
        }
        let eq = s.find('=')?;
        let key = s[..eq].trim();
        s = s[eq + 1..].trim_start().strip_prefix('"')?;
        let mut value = String::new();
        let mut chars = s.char_indices();
        let end = loop {
            match chars.next()? {
                (i, '"') => break i,
                (_, '\\') => match chars.next()?.1 {
                    'n' => value.push('\n'),
                    other => value.push(other),
                },
                (_, c) => value.push(c),
            }
        };
        labels.push((key, value));
        s = &s[end + 1..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MICROMETER: &str = r#"# HELP process_cpu_usage The "recent cpu usage" for the Java Virtual Machine process
# TYPE process_cpu_usage gauge
process_cpu_usage 0.25
# TYPE system_cpu_count gauge
system_cpu_count 4.0
# TYPE jvm_memory_used_bytes gauge
jvm_memory_used_bytes{area="heap",id="G1 Eden Space",} 1.048576E8
jvm_memory_used_bytes{area="heap",id="G1 Old Gen",} 5.0E7
jvm_memory_used_bytes{area="nonheap",id="Metaspace",} 6.0E7
jvm_memory_max_bytes{area="heap",id="G1 Eden Space",} -1.0
jvm_memory_max_bytes{area="heap",id="G1 Old Gen",} 5.36870912E8
jvm_memory_max_bytes{area="nonheap",id="Metaspace",} -1.0
jvm_gc_pause_seconds_count{action="end of minor GC",cause="G1 Evacuation Pause",gc="G1 Young Generation",} 12.0
jvm_gc_pause_seconds_sum{action="end of minor GC",cause="G1 Evacuation Pause",gc="G1 Young Generation",} 0.12
jvm_gc_pause_seconds_sum{action="end of major GC",cause="System.gc()",gc="G1 Old Generation",} 0.03
http_server_requests_seconds_count{method="GET",uri="/a \"b\"",} 3.0
"#;

    const STANDARD: &str = "process_cpu_seconds_total 12.5\nprocess_resident_memory_bytes 2.5e+08\nnodejs_gc_duration_seconds_sum{kind=\"minor\"} 0.5\n";

    #[test]
    fn parses_names_labels_and_values() {
        let metrics: Vec<_> = parse_prometheus(MICROMETER).collect();
        assert_eq!(metrics[0].name, "process_cpu_usage");
        assert_eq!(metrics[0].value, 0.25);
        let eden = &metrics[2];
        assert_eq!(eden.label("area"), Some("heap"));
        assert_eq!(eden.label("id"), Some("G1 Eden Space"));
        assert_eq!(eden.value, 104_857_600.0);
        let escaped = metrics.last().unwrap();
        assert_eq!(escaped.label("uri"), Some("/a \"b\""));
    }

    #[test]
    fn reads_micrometer() {
        let f = Figures::from_prometheus(MICROMETER);
        assert_eq!(f.cpu_usage, Some(0.25));
        assert_eq!(f.cpu_count, Some(4.0));
        assert_eq!(f.heap_used, Some(154_857_600.0));
        assert_eq!(f.jvm_used, Some(214_857_600.0));
        // -1 means "no limit" and is left out.
        assert_eq!(f.heap_max, Some(536_870_912.0));
        assert_eq!(f.counters.gc_seconds, Some(0.15));
        let read = f.to_read(0.0, None, Mode::Prometheus);
        // A quarter of 4 cores is one full core.
        assert_eq!(read.sample.cpu_percent, Some(100.0));
        assert_eq!(read.memory_kind, Some("jvm"));
        assert_eq!(read.sample.gc_ms_per_s, None);
    }

    #[test]
    fn rates_come_from_counters_between_reads() {
        let first = Figures::from_prometheus(STANDARD);
        assert_eq!(first.resident, Some(250_000_000.0));
        let read = first.to_read(0.0, None, Mode::Prometheus);
        assert_eq!(read.sample.cpu_percent, None);
        assert_eq!(read.memory_kind, Some("resident"));

        let later = Figures::from_prometheus(
            "process_cpu_seconds_total 14\nprocess_resident_memory_bytes 3e8\nnodejs_gc_duration_seconds_sum 0.52\n",
        );
        let read = later.to_read(2.0, Some((2.0, &first.counters)), Mode::Prometheus);
        // 1.5 CPU seconds in 2 seconds: three quarters of a core.
        assert_eq!(read.sample.cpu_percent, Some(75.0));
        assert_eq!(read.sample.gc_ms_per_s, Some(10.0));
        assert_eq!(read.sample.memory_bytes, Some(300_000_000.0));

        // A restart resets the counters: no rate rather than a negative one.
        let restarted = Figures::from_prometheus("process_cpu_seconds_total 1\n");
        let read = restarted.to_read(4.0, Some((2.0, &later.counters)), Mode::Prometheus);
        assert_eq!(read.sample.cpu_percent, None);
    }

    #[test]
    fn actuator_json_measurements() {
        let body = r#"{"name":"jvm.gc.pause","measurements":[{"statistic":"COUNT","value":3.0},{"statistic":"TOTAL_TIME","value":0.25},{"statistic":"MAX","value":0.1}]}"#;
        assert_eq!(actuator_value(body, "TOTAL_TIME"), Ok(0.25));
        let body =
            r#"{"name":"process.cpu.usage","measurements":[{"statistic":"VALUE","value":0.5}]}"#;
        assert_eq!(actuator_value(body, "VALUE"), Ok(0.5));
        assert!(actuator_value("<html>", "VALUE").is_err());
    }

    #[test]
    fn stats_sum_up_a_series() {
        let sample = |t, cpu, mem| ResourceSample {
            t,
            cpu_percent: cpu,
            memory_bytes: Some(mem),
            ..ResourceSample::default()
        };
        let series = ResourceSeries {
            name: "api".into(),
            url: "http://api/metrics".into(),
            format: "prometheus".into(),
            cpu_cores: None,
            heap_max_bytes: None,
            memory_kind: None,
            samples: vec![
                sample(0.0, None, 100.0),
                sample(1.0, Some(50.0), 300.0),
                sample(2.0, Some(150.0), 200.0),
            ],
            failed_reads: 0,
            error: None,
            health_url: None,
            health: Vec::new(),
            health_error: None,
            failing: false,
        };
        let stats = series.stats();
        assert_eq!(stats.cpu_avg, Some(100.0));
        assert_eq!(stats.cpu_peak, Some(150.0));
        assert_eq!(stats.memory_start, Some(100.0));
        assert_eq!(stats.memory_peak, Some(300.0));
        assert_eq!(stats.memory_end, Some(200.0));
    }

    #[test]
    fn health_answers() {
        use reqwest::StatusCode;
        let down = r#"{"status":"DOWN","components":{"db":{"status":"DOWN","details":{"error":"timeout"}},"diskSpace":{"status":"UP"},"ping":{"status":"UP"},"redis":{"status":"OUT_OF_SERVICE","components":{"primary":{"status":"DOWN"},"replica":{"status":"UP"}}}}}"#;
        assert_eq!(
            HealthOutcome::from_response(StatusCode::SERVICE_UNAVAILABLE, down),
            HealthOutcome::Status("DOWN".into(), Some("db: DOWN, redis.primary: DOWN".into()))
        );
        assert_eq!(
            HealthOutcome::from_response(StatusCode::OK, r#"{"status":"UP"}"#),
            HealthOutcome::Status("UP".into(), None)
        );
        // Plain health checks: the status code is all there is.
        assert_eq!(
            HealthOutcome::from_response(StatusCode::OK, "ok"),
            HealthOutcome::Status("UP".into(), None)
        );
        assert_eq!(
            HealthOutcome::from_response(StatusCode::BAD_GATEWAY, "<html>"),
            HealthOutcome::Status("HTTP 502".into(), Some("502 Bad Gateway".into()))
        );
    }

    #[test]
    fn unhealthy_periods_join_unhealthy_states() {
        let change = |t, status: &str| HealthChange {
            t,
            status: status.into(),
            detail: None,
        };
        let series = ResourceSeries {
            name: "api".into(),
            url: "http://api/metrics".into(),
            format: "prometheus".into(),
            cpu_cores: None,
            heap_max_bytes: None,
            memory_kind: None,
            samples: Vec::new(),
            failed_reads: 0,
            error: None,
            health_url: Some("http://api/health".into()),
            health: vec![
                change(0.0, "UP"),
                change(3.0, "DOWN"),
                change(4.0, "UNREACHABLE"),
                change(6.0, "UP"),
                change(8.0, "UNKNOWN"),
                change(9.0, "OUT_OF_SERVICE"),
            ],
            health_error: None,
            failing: false,
        };
        let periods = series.unhealthy_periods();
        assert_eq!(periods.len(), 2);
        assert_eq!((periods[0].from, periods[0].to), (3.0, Some(6.0)));
        assert_eq!(periods[0].status, "DOWN");
        assert_eq!((periods[1].from, periods[1].to), (9.0, None));
    }

    #[test]
    fn reports_show_no_secrets_in_urls() {
        let url = reqwest::Url::parse("https://user:pw@api.test/metrics?token=x").unwrap();
        assert_eq!(shown(&url), "https://user@api.test/metrics");
    }
}
