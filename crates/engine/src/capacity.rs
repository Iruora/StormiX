//! The result of a `find_limit` load test, and how many pods that limit
//! means for the traffic in the test file's `capacity` section.
//!
//! The sizing works from the last step where every threshold held:
//!
//! - usable rate per pod = rate × min(1, target CPU ÷ CPU at that rate),
//!   so a pod runs at the target CPU at most, and never above a rate it was
//!   seen to hold. CPU only grows in step with the rate until the pod is
//!   saturated, so when the sustained step ran at 90% or more of the pod,
//!   the highest step that held below 90% is scaled instead;
//! - pods = ⌈peak × (1 + growth) ÷ usable rate per pod⌉ + spare pods.
//!
//! Without a CPU reading the step is taken as saturated, so the usable rate
//! is that rate × the target CPU.
//!
//! Memory is sized too when the pod's memory limit is given. A pod's memory
//! is about a fixed part (runtime, code, caches) plus a share per request in
//! flight, so a straight line through each step's (requests in flight,
//! memory) gives both. The traffic then needs
//! ⌈design rate × latency × memory per request ÷ (limit × target − fixed)⌉
//! pods for its in-flight requests to fit, and the answer is the larger of
//! the CPU and memory counts. Little's law (in flight = rate × time in
//! the system) gives the requests one pod has in flight at its usable rate,
//! a floor for its thread and connection pools.

use serde::{Deserialize, Serialize};
use stormix_flow::Capacity;
use stormix_metrics::{Latency, ThresholdResult};

use crate::monitor::ResourceSeries;

/// One step of a `find_limit` test, measured on its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LimitStep {
    /// The flow iterations started per second during the step.
    pub rate: u32,
    /// Seconds since the load started.
    pub start_s: f64,
    pub end_s: f64,
    /// Requests per second the API answered during the step, every step
    /// of the flow counted.
    pub rps: f64,
    pub requests: u32,
    pub failed_requests: u32,
    /// Failed requests, in percent.
    pub error_rate: f64,
    pub latency: Latency,
    /// The API's average CPU during the step, where 100 is one core. The
    /// busiest monitored API's when there are several.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_cpu: Option<f64>,
    /// The API's average memory during the step, in bytes, as the monitor
    /// measures it (see `ResourceSeries::memory_kind`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_memory_bytes: Option<f64>,
    /// The thresholds checked against this step alone (`rps` ones are
    /// left out: the step sets the rate).
    pub thresholds: Vec<ThresholdResult>,
    pub passed: bool,
    /// Cut short by a stop, so it decides nothing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub partial: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LimitSearch {
    /// In the order they ran.
    pub steps: Vec<LimitStep>,
    /// The index in `steps` of the last step where every threshold held:
    /// the most the API sustained. None when even the first step failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sustained: Option<u32>,
    /// A step broke a threshold. When false the max rate held, so the API
    /// can take at least that much and the real limit is higher.
    pub reached: bool,
    /// The monitored API's CPU cores, when it says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<f64>,
    /// How many pods the traffic in the `capacity` section needs. None
    /// without that section, or when no step held.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sizing: Option<Sizing>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Sizing {
    /// What it was sized for.
    pub capacity: Capacity,
    /// Requests per second one pod held at the sustained step.
    pub tested_rps_per_pod: f64,
    /// The pod's CPU at that step, in percent of its cores. None without a
    /// monitor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tested_cpu: Option<f64>,
    /// The step the CPU was scaled from, an index in `steps`: the sustained
    /// one, or a lower one when the sustained one saturated the CPU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_step: Option<u32>,
    /// The cores used to turn the API's CPU into a share of the pod.
    pub pod_cores: f64,
    /// Requests per second one pod should take at the peak.
    pub usable_rps_per_pod: f64,
    /// The peak plus growth.
    pub design_rps: f64,
    /// Pods the traffic needs, before the spare ones.
    pub pods_for_traffic: u32,
    /// Including the spare pods: the answer.
    pub pods: u32,
    /// Average latency at the sustained step, in milliseconds.
    pub latency_ms: f64,
    /// Requests one pod has in flight at its usable rate (Little's law).
    pub in_flight_per_pod: f64,
    /// Requests all pods have in flight at the design rate.
    pub in_flight_total: f64,
    /// Pods the CPU needs, before the spare ones.
    pub pods_by_cpu: u32,
    /// Pods the memory needs, before the spare ones. None without
    /// `pod_memory_mb`, or when the steps say too little about memory
    /// (see `memory_note`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pods_by_memory: Option<u32>,
    /// A pod's memory that does not depend on load, in bytes, from the fit
    /// across the steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_memory_bytes: Option<f64>,
    /// The memory each request in flight adds, in bytes, from the fit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_per_request_bytes: Option<f64>,
    /// Why memory was not sized, or what to watch: too few steps with a
    /// memory reading, memory that did not grow with load, or a fixed part
    /// over the target already.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_note: Option<String>,
    /// How the usable rate was found: `cpu` (scaled to the target CPU),
    /// `latency` (CPU was below the target when a threshold broke, so the
    /// tested rate is the cap) or `assumed` (no CPU reading; the step is
    /// taken as saturated).
    pub basis: String,
}

/// The share of a pod's CPU, in percent, from which it counts as saturated.
const SATURATED: f64 = 90.0;

/// The pods `capacity` needs, from the sustained step of `limit`. None
/// when no step held.
pub fn size(limit: &LimitSearch, capacity: &Capacity) -> Option<Sizing> {
    let step = limit.steps.get(limit.sustained? as usize)?;
    let tested_pods = f64::from(capacity.tested_pods.max(1));
    let tested_rps_per_pod = step.rps / tested_pods;
    if tested_rps_per_pod <= 0.0 {
        return None;
    }
    let pod_cores = capacity
        .pod_cores
        .or(limit.cpu_cores)
        .filter(|c| *c > 0.0)
        .unwrap_or(1.0);
    let target = capacity.target_cpu.clamp(1.0, 100.0);
    let share = |s: &LimitStep| s.api_cpu.map(|cpu| cpu / pod_cores);
    let tested_cpu = share(step);
    let sustained = limit.sustained? as usize;
    // Past saturation the CPU stays near 100% whatever the rate, so it
    // says nothing about how far the rate can go.
    let cpu_step = match tested_cpu {
        Some(cpu) if cpu >= SATURATED => limit.steps[..sustained]
            .iter()
            .rposition(|s| s.passed && share(s).is_some_and(|c| c > 0.0 && c < SATURATED))
            .or(Some(sustained)),
        Some(_) => Some(sustained),
        None => None,
    };
    let (usable_rps_per_pod, basis) = match cpu_step.map(|i| &limit.steps[i]) {
        Some(reference) => {
            let cpu = share(reference).unwrap_or(100.0);
            let rps = reference.rps / tested_pods;
            if cpu > target {
                ((rps * target / cpu).min(tested_rps_per_pod), "cpu")
            } else {
                (tested_rps_per_pod, "latency")
            }
        }
        None => (tested_rps_per_pod * target / 100.0, "assumed"),
    };
    let design_rps = capacity.peak_rps.max(0.0) * (1.0 + capacity.growth.max(0.0) / 100.0);
    let pods_by_cpu = ((design_rps / usable_rps_per_pod).ceil() as u32).max(1);
    let latency_ms = step.latency.avg;
    let memory = memory_pods(limit, capacity, design_rps * latency_ms / 1000.0);
    let pods_for_traffic = pods_by_cpu.max(memory.pods.unwrap_or(0));
    Some(Sizing {
        capacity: capacity.clone(),
        tested_rps_per_pod,
        tested_cpu,
        cpu_step: cpu_step.map(|i| i as u32),
        pod_cores,
        usable_rps_per_pod,
        design_rps,
        pods_for_traffic,
        pods: pods_for_traffic.saturating_add(capacity.spare_pods),
        latency_ms,
        in_flight_per_pod: usable_rps_per_pod * latency_ms / 1000.0,
        in_flight_total: design_rps * latency_ms / 1000.0,
        pods_by_cpu,
        pods_by_memory: memory.pods,
        fixed_memory_bytes: memory.fixed,
        memory_per_request_bytes: memory.per_request,
        memory_note: memory.note,
        basis: basis.into(),
    })
}

#[derive(Default)]
struct MemoryFit {
    pods: Option<u32>,
    fixed: Option<f64>,
    per_request: Option<f64>,
    note: Option<String>,
}

const MIB: f64 = 1024.0 * 1024.0;

/// Fits memory = fixed + per request × requests in flight through the
/// steps that ran to the end, one pod's share each, then sizes for
/// `in_flight_total` requests in flight across all pods.
fn memory_pods(limit: &LimitSearch, capacity: &Capacity, in_flight_total: f64) -> MemoryFit {
    let Some(limit_mb) = capacity.pod_memory_mb.filter(|m| m.is_finite() && *m > 0.0) else {
        return MemoryFit::default();
    };
    let tested_pods = f64::from(capacity.tested_pods.max(1));
    let points: Vec<(f64, f64)> = limit
        .steps
        .iter()
        .filter(|s| !s.partial)
        .filter_map(|s| {
            let in_flight = s.rps * s.latency.avg / 1000.0 / tested_pods;
            s.api_memory_bytes.map(|m| (in_flight, m))
        })
        .collect();
    let note = |text: &str| MemoryFit {
        note: Some(text.to_owned()),
        ..MemoryFit::default()
    };
    if points.len() < 2 {
        return note("fewer than two steps read the API's memory; add a monitor to size memory");
    }
    let n = points.len() as f64;
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    let sxy: f64 = points.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_y)).sum();
    let per_request = if sxx > 0.0 { (sxy / sxx).max(0.0) } else { 0.0 };
    let fixed = (mean_y - per_request * mean_x).max(0.0);
    let budget = limit_mb * MIB * capacity.target_memory.clamp(1.0, 100.0) / 100.0;
    let mut fit = MemoryFit {
        fixed: Some(fixed),
        per_request: Some(per_request),
        ..MemoryFit::default()
    };
    if fixed >= budget {
        fit.note = Some(
            "the fixed part alone is over the memory target, so more pods will not help; raise the memory limit or find what holds it (a cache, a leak)".into(),
        );
        return fit;
    }
    if per_request == 0.0 {
        fit.pods = Some(1);
        fit.note = Some(
            "memory did not grow with the requests in flight, so it does not limit the pod count"
                .into(),
        );
        return fit;
    }
    fit.pods = Some((((in_flight_total * per_request) / (budget - fixed)).ceil() as u32).max(1));
    fit
}

/// The busiest API's average memory from `start_s` to `end_s`.
pub(crate) fn average_memory(
    resources: &[ResourceSeries],
    start_s: f64,
    end_s: f64,
) -> Option<f64> {
    resources
        .iter()
        .filter_map(|series| {
            let memory: Vec<f64> = series
                .samples
                .iter()
                .filter(|s| s.t > start_s && s.t <= end_s)
                .filter_map(|s| s.memory_bytes)
                .collect();
            (!memory.is_empty()).then(|| memory.iter().sum::<f64>() / memory.len() as f64)
        })
        .max_by(f64::total_cmp)
}

/// The busiest API's average CPU from `start_s` to `end_s` (seconds since
/// the load started). None when no monitor read CPU in that time.
pub(crate) fn average_cpu(resources: &[ResourceSeries], start_s: f64, end_s: f64) -> Option<f64> {
    resources
        .iter()
        .filter_map(|series| {
            let cpu: Vec<f64> = series
                .samples
                .iter()
                .filter(|s| s.t > start_s && s.t <= end_s)
                .filter_map(|s| s.cpu_percent)
                .collect();
            (!cpu.is_empty()).then(|| cpu.iter().sum::<f64>() / cpu.len() as f64)
        })
        .max_by(f64::total_cmp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(rate: u32, rps: f64, cpu: Option<f64>, avg_ms: f64, passed: bool) -> LimitStep {
        LimitStep {
            rate,
            start_s: 0.0,
            end_s: 30.0,
            rps,
            requests: 0,
            failed_requests: 0,
            error_rate: 0.0,
            latency: Latency {
                avg: avg_ms,
                ..Latency::default()
            },
            api_cpu: cpu,
            api_memory_bytes: None,
            thresholds: Vec::new(),
            passed,
            partial: false,
        }
    }

    fn capacity(peak: f64) -> Capacity {
        stormix_flow::TestFile::from_yaml(&format!(
            "version: 1\nflow: {{ name: x, steps: [] }}\ncapacity: {{ peak_rps: {peak} }}\n"
        ))
        .unwrap()
        .capacity
        .unwrap()
    }

    fn search(steps: Vec<LimitStep>, sustained: Option<u32>, cores: Option<f64>) -> LimitSearch {
        LimitSearch {
            steps,
            sustained,
            reached: true,
            cpu_cores: cores,
            sizing: None,
        }
    }

    #[test]
    fn scales_the_sustained_rate_to_the_target_cpu() {
        // 200 req/s at 90% of 2 cores; 65% target -> 144.4 usable.
        let limit = search(
            vec![
                step(100, 200.0, Some(180.0), 40.0, true),
                step(150, 260.0, Some(199.0), 900.0, false),
            ],
            Some(0),
            Some(2.0),
        );
        let s = size(&limit, &capacity(1100.0)).unwrap();
        assert_eq!(s.basis, "cpu");
        assert!((s.tested_cpu.unwrap() - 90.0).abs() < 1e-9);
        assert!((s.usable_rps_per_pod - 200.0 * 65.0 / 90.0).abs() < 1e-9);
        // 1100 × 1.3 = 1430 / 144.4 = 9.9 -> 10, + 1 spare.
        assert!((s.design_rps - 1430.0).abs() < 1e-9);
        assert_eq!(s.pods_for_traffic, 10);
        assert_eq!(s.pods, 11);
        assert!((s.in_flight_per_pod - s.usable_rps_per_pod * 0.04).abs() < 1e-9);
    }

    #[test]
    fn a_saturated_step_scales_from_the_last_one_below_saturation() {
        let limit = search(
            vec![
                step(10, 20.0, Some(40.0), 10.0, true),
                step(20, 40.0, Some(80.0), 10.0, true),
                step(30, 60.0, Some(99.0), 12.0, true),
                step(40, 80.0, Some(100.0), 15.0, true),
            ],
            Some(3),
            Some(1.0),
        );
        let s = size(&limit, &capacity(100.0)).unwrap();
        assert_eq!(s.cpu_step, Some(1));
        assert_eq!(s.basis, "cpu");
        // 40 req/s at 80% -> 32.5 at 65%.
        assert!((s.usable_rps_per_pod - 32.5).abs() < 1e-9);
    }

    #[test]
    fn memory_is_fitted_across_the_steps_and_can_need_more_pods_than_cpu() {
        // 100 MiB fixed + 10 MiB per request in flight. rps × 100 ms gives
        // 1, 2, 3 and 4 in flight.
        let mut steps = vec![
            step(10, 10.0, Some(20.0), 100.0, true),
            step(20, 20.0, Some(40.0), 100.0, true),
            step(30, 30.0, Some(60.0), 100.0, true),
            step(40, 40.0, Some(80.0), 100.0, false),
        ];
        for s in &mut steps {
            s.api_memory_bytes = Some((100.0 + 10.0 * s.rps / 10.0) * MIB);
        }
        let limit = search(steps, Some(2), Some(1.0));
        let mut c = capacity(300.0);
        c.growth = stormix_flow::Percent(0.0);
        c.pod_memory_mb = Some(200.0);
        let s = size(&limit, &c).unwrap();
        assert!((s.fixed_memory_bytes.unwrap() - 100.0 * MIB).abs() < 1.0);
        assert!((s.memory_per_request_bytes.unwrap() - 10.0 * MIB).abs() < 1.0);
        // 300 req/s × 0.1 s = 30 in flight × 10 MiB = 300 MiB over pods
        // with 200 × 80% − 100 = 60 MiB each to spare: 5 pods. CPU: 30
        // req/s at 60% -> 32.5 at 65%, capped at 30: 10 pods.
        assert_eq!(s.pods_by_memory, Some(5));
        assert_eq!(s.pods_by_cpu, 10);
        assert_eq!(s.pods_for_traffic, 10);
        c.pod_memory_mb = Some(120.0);
        let s = size(&limit, &c).unwrap();
        assert_eq!(s.pods_by_memory, None);
        assert!(s.memory_note.unwrap().contains("fixed part"));
    }

    #[test]
    fn a_latency_limit_below_the_target_cpu_caps_at_the_tested_rate() {
        let limit = search(vec![step(50, 100.0, Some(30.0), 20.0, true)], Some(0), None);
        let s = size(&limit, &capacity(250.0)).unwrap();
        assert_eq!(s.basis, "latency");
        assert_eq!(s.usable_rps_per_pod, 100.0);
        assert_eq!(s.pods, 5); // ceil(325 / 100) + 1
    }

    #[test]
    fn without_cpu_the_step_is_taken_as_saturated() {
        let limit = search(vec![step(50, 100.0, None, 20.0, true)], Some(0), None);
        let s = size(&limit, &capacity(65.0)).unwrap();
        assert_eq!(s.basis, "assumed");
        assert_eq!(s.usable_rps_per_pod, 65.0);
        assert_eq!(s.pods, 3); // ceil(84.5 / 65) + 1
    }

    #[test]
    fn nothing_to_size_when_no_step_held() {
        let limit = search(vec![step(50, 100.0, None, 20.0, false)], None, None);
        assert!(size(&limit, &capacity(65.0)).is_none());
    }
}
