//! Self-contained HTML reports: one file, no scripts and no network, that
//! opens in any browser and can be attached to a CI run or an email.
//!
//! [`flow_report`] covers a functional run, [`load_report`] a load test.

use std::fmt::Write;

use stormix_engine::{FlowResult, LoadReport, StepResult};
use stormix_metrics::{Second, ThresholdResult};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A functional run: the outcome, then each step with its checks.
pub fn flow_report(result: &FlowResult) -> String {
    let (checks_passed, checks) = result.check_counts();
    let ran = result.steps.iter().filter(|s| !s.skipped).count();
    let passed = result.passed();
    let mut body = String::new();

    let _ = write!(
        body,
        r#"<header class="{tone}"><div class="verdict">{icon} {verdict}</div><h1>{name}</h1>
<div class="tiles">{t1}{t2}{t3}{t4}</div></header>"#,
        tone = if passed { "ok" } else { "bad" },
        icon = if passed { "✓" } else { "✗" },
        verdict = if passed { "Run passed" } else { "Run failed" },
        name = esc(&result.name),
        t1 = tile(
            "steps passed",
            &format!("{}/{}", ran - result.failed_steps(), result.steps.len())
        ),
        t2 = tile("checks passed", &format!("{checks_passed}/{checks}")),
        t3 = tile("duration", &ms(result.duration_ms)),
        t4 = tile("failed steps", &result.failed_steps().to_string()),
    );

    body.push_str("<section><h2>Steps</h2><ol class=\"steps\">");
    for step in &result.steps {
        step_html(&mut body, step);
    }
    body.push_str("</ol></section>");
    page(&format!("{} · Stormix run", result.name), &body)
}

fn step_html(out: &mut String, step: &StepResult) {
    let (class, icon) = if step.skipped {
        ("skip", "–")
    } else if step.passed() {
        ("ok", "✓")
    } else {
        ("bad", "✗")
    };
    let status = match (step.skipped, step.status) {
        (true, _) => "skipped".to_owned(),
        (_, Some(s)) => s.to_string(),
        (_, None) => "no response".to_owned(),
    };
    let _ = write!(
        out,
        r#"<li class="{class}"><div class="row"><span class="icon">{icon}</span><span class="name">{name}</span><span class="status">{status}</span><span class="num">{dur}</span></div><div class="url mono">{method} {url}</div>"#,
        name = esc(&step.name),
        status = esc(&status),
        dur = if step.skipped {
            String::new()
        } else {
            ms(step.duration_ms)
        },
        method = esc(&step.method),
        url = esc(&step.url),
    );
    if let Some(trace_id) = &step.trace_id {
        let _ = write!(
            out,
            r#"<div class="url mono">trace <span class="trace">{}</span></div>"#,
            esc(trace_id)
        );
    }
    if let Some(error) = &step.error {
        let _ = write!(out, r#"<div class="error">{}</div>"#, esc(error));
    }
    if !step.checks.is_empty() {
        out.push_str("<ul class=\"checks\">");
        for c in &step.checks {
            let _ = write!(
                out,
                r#"<li class="{}">{} {}{}</li>"#,
                if c.passed { "ok" } else { "bad" },
                if c.passed { "✓" } else { "✗" },
                esc(&c.name),
                c.message
                    .as_ref()
                    .map(|m| format!(r#"<span class="muted">: {}</span>"#, esc(m)))
                    .unwrap_or_default(),
            );
        }
        out.push_str("</ul>");
    }
    for w in &step.warnings {
        let _ = write!(out, r#"<div class="warn">! {}</div>"#, esc(w));
    }
    if !step.logs.is_empty() {
        let _ = write!(
            out,
            r#"<details><summary>console ({})</summary><pre>{}</pre></details>"#,
            step.logs.len(),
            esc(&step.logs.join("\n"))
        );
    }
    out.push_str("</li>");
}

/// A load test: headline figures, thresholds, a timeline, each step and the
/// most common failures.
pub fn load_report(report: &LoadReport) -> String {
    let s = &report.summary;
    let l = &s.latency;
    let passed = report.passed();
    let mut body = String::new();
    let verdict = if report.thresholds.is_empty() {
        "Load test finished"
    } else if passed {
        "Thresholds held"
    } else {
        "Thresholds failed"
    };

    let _ = write!(
        body,
        r#"<header class="{tone}"><div class="verdict">{icon} {verdict}</div><h1>{name}</h1>
<div class="tiles">{t1}{t2}{t3}{t4}{t5}</div>
<div class="mono muted latency">min {min} · avg {avg} · p50 {p50} · p90 {p90} · p95 {p95} · p99 {p99} · max {max}</div></header>"#,
        tone = if passed { "ok" } else { "bad" },
        icon = if passed { "✓" } else { "✗" },
        name = esc(&report.name),
        t1 = tile("requests", &s.requests.to_string()),
        t2 = tile("req/s", &format!("{:.1}", s.rps)),
        t3 = tile("p95", &ms(l.p95)),
        t4 = tile("failed", &format!("{:.2}%", s.error_rate())),
        t5 = tile("checks passed", &format!("{:.2}%", s.check_rate())),
        min = ms(l.min),
        avg = ms(l.avg),
        p50 = ms(l.p50),
        p90 = ms(l.p90),
        p95 = ms(l.p95),
        p99 = ms(l.p99),
        max = ms(l.max),
    );

    if !report.thresholds.is_empty() {
        body.push_str("<section><h2>Thresholds</h2><ul class=\"thresholds\">");
        for t in &report.thresholds {
            threshold_html(&mut body, t);
        }
        body.push_str("</ul></section>");
    }

    if s.timeline.len() > 1 {
        let _ = write!(
            body,
            "<section><h2>Over time</h2>{}</section>",
            timeline_svg(&s.timeline)
        );
    }

    let _ = write!(
        body,
        r#"<section><h2>Iterations</h2><p>{} iterations, {} failed{}{} · {} received</p></section>"#,
        s.iterations,
        s.failed_iterations,
        if s.dropped_iterations > 0 {
            format!(
                ", {} dropped because too many were in flight",
                s.dropped_iterations
            )
        } else {
            String::new()
        },
        if s.interrupted_iterations > 0 {
            format!(", {} cut off at the end", s.interrupted_iterations)
        } else {
            String::new()
        },
        bytes(s.bytes_received),
    );

    if !s.steps.is_empty() {
        body.push_str(
            r#"<section><h2>Steps</h2><table><thead><tr><th>Step</th><th>Requests</th><th>Failed</th><th>p50</th><th>p95</th><th>p99</th><th>max</th></tr></thead><tbody>"#,
        );
        for step in &s.steps {
            let _ = write!(
                body,
                r#"<tr><td>{}</td><td>{}</td><td class="{}">{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
                esc(&step.name),
                step.requests,
                if step.failed > 0 { "bad" } else { "" },
                step.failed,
                ms(step.latency.p50),
                ms(step.latency.p95),
                ms(step.latency.p99),
                ms(step.latency.max),
            );
        }
        body.push_str("</tbody></table></section>");
    }

    if !s.failures.is_empty() {
        body.push_str("<section><h2>Most common failures</h2><ul class=\"failures\">");
        for f in &s.failures {
            let _ = write!(
                body,
                r#"<li><span class="mono bad">{}×</span> {}</li>"#,
                f.count,
                esc(&f.reason)
            );
        }
        body.push_str("</ul></section>");
    }

    if !report.slowest.is_empty() {
        body.push_str(
            r#"<section><h2>Slowest traced requests</h2><p class="muted">Search for a trace ID in Dynatrace or another APM tool to see where the time went.</p><table><thead><tr><th>Step</th><th>Time</th><th>Status</th><th>Trace ID</th></tr></thead><tbody>"#,
        );
        for r in &report.slowest {
            let _ = write!(
                body,
                r#"<tr><td>{}</td><td>{}</td><td class="{}">{}</td><td class="trace">{}</td></tr>"#,
                esc(&r.step),
                ms(r.duration_ms),
                if r.passed { "" } else { "bad" },
                r.status.map_or("—".to_owned(), |s| s.to_string()),
                esc(&r.trace_id),
            );
        }
        body.push_str("</tbody></table></section>");
    }
    page(&format!("{} · Stormix load test", report.name), &body)
}

fn threshold_html(out: &mut String, t: &ThresholdResult) {
    let metric = t
        .threshold
        .split([' ', '<', '>'])
        .next()
        .unwrap_or_default();
    let actual = match metric {
        "errors" | "checks" => format!("{:.2}%", t.actual),
        "rps" => format!("{:.1}/s", t.actual),
        _ => ms(t.actual),
    };
    let _ = write!(
        out,
        r#"<li class="{}"><span class="icon">{}</span><span class="mono">{}</span><span class="mono num">{}</span></li>"#,
        if t.passed { "ok" } else { "bad" },
        if t.passed { "✓" } else { "✗" },
        esc(&t.threshold),
        actual,
    );
}

/// Requests per second as bars (failures stacked in red) and p95 as a line.
fn timeline_svg(seconds: &[Second]) -> String {
    const W: f64 = 800.0;
    const H: f64 = 160.0;
    let max_req = seconds.iter().map(|s| s.requests).max().unwrap_or(0).max(1) as f64;
    let max_p95 = seconds
        .iter()
        .map(|s| s.p95_ms)
        .fold(0.0, f64::max)
        .max(0.001);
    let bar = W / seconds.len() as f64;
    let y = |v: f64, max: f64| H - v / max * (H - 8.0);

    let mut svg = format!(
        r#"<div class="legend"><span><i class="sw req"></i>req/s (max {})</span><span><i class="sw fail"></i>failed</span><span><i class="sw p95"></i>p95 (max {})</span></div><svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" role="img" aria-label="Requests per second and p95 latency over time">"#,
        max_req as u32,
        ms(max_p95),
    );
    for (i, s) in seconds.iter().enumerate() {
        let x = i as f64 * bar + 0.5;
        let w = (bar - 1.0).max(0.5);
        let top = y(s.requests as f64, max_req);
        let _ = write!(
            svg,
            r#"<rect class="req" x="{x:.1}" y="{top:.1}" width="{w:.1}" height="{:.1}"/>"#,
            H - top
        );
        if s.failed > 0 {
            let ok_top = y((s.requests - s.failed) as f64, max_req);
            let _ = write!(
                svg,
                r#"<rect class="fail" x="{x:.1}" y="{top:.1}" width="{w:.1}" height="{:.1}"/>"#,
                ok_top - top
            );
        }
    }
    let points: Vec<String> = seconds
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{:.1},{:.1}", (i as f64 + 0.5) * bar, y(s.p95_ms, max_p95)))
        .collect();
    let _ = write!(
        svg,
        r#"<polyline class="p95" points="{}"/></svg><div class="axis mono"><span>{}s</span><span>{}s</span><span>{}s</span></div>"#,
        points.join(" "),
        seconds[0].t,
        seconds[seconds.len() / 2].t,
        seconds[seconds.len() - 1].t,
    );
    svg
}

fn tile(label: &str, value: &str) -> String {
    format!(
        r#"<div class="tile"><div class="value">{}</div><div class="label">{}</div></div>"#,
        esc(value),
        esc(label)
    )
}

fn page(title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="generator" content="Stormix {VERSION}">
<title>{title}</title><style>{CSS}</style></head>
<body><main>{body}<footer>Made with Stormix {VERSION}</footer></main></body></html>
"#,
        title = esc(title),
    )
}

const CSS: &str = r#"
:root{--bg:#f4f5f9;--surface:#fff;--line:#e2e5ee;--ink:#161a2b;--muted:#5f667d;--accent:#4f46e5;--ok:#15803d;--ok-soft:#e7f6ec;--bad:#b91c1c;--bad-soft:#fdeaea;--warn:#b45309;color-scheme:light}
@media (prefers-color-scheme:dark){:root{--bg:#0e1120;--surface:#161a2c;--line:#2a3048;--ink:#e8eaf3;--muted:#a0a6bd;--accent:#8b85ff;--ok:#4ade80;--ok-soft:#12301f;--bad:#f87171;--bad-soft:#3a1717;--warn:#fbbf24;color-scheme:dark}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.5 system-ui,-apple-system,"Segoe UI",sans-serif}
main{max-width:960px;margin:0 auto;padding:32px 16px}
.mono,pre,.url,.status,.num,td{font-family:ui-monospace,"SF Mono",Menlo,Consolas,monospace;font-size:13px}
header,section{background:var(--surface);border:1px solid var(--line);border-radius:12px;padding:20px 24px;margin-bottom:16px}
header{border-top:4px solid var(--ok)}header.bad{border-top-color:var(--bad)}
.verdict{font-weight:700;color:var(--ok)}header.bad .verdict{color:var(--bad)}
h1{margin:4px 0 16px;font-size:24px}h2{margin:0 0 12px;font-size:16px}
.tiles{display:grid;grid-template-columns:repeat(auto-fit,minmax(120px,1fr));gap:12px}
.tile .value{font-size:22px;font-weight:650;font-variant-numeric:tabular-nums}.tile .label,.muted,.url{color:var(--muted)}
.latency{margin-top:12px}
ol.steps,ul.checks,ul.thresholds,ul.failures{list-style:none;margin:0;padding:0}
ol.steps>li{border-top:1px solid var(--line);padding:10px 0}ol.steps>li:first-child{border-top:0}
.row{display:flex;gap:10px;align-items:baseline}.name{font-weight:600;flex:1;min-width:0}
.icon{width:1em;font-weight:700}li.ok>.row .icon,li.ok>.icon,ul.checks li.ok{color:var(--ok)}li.bad>.row .icon,li.bad>.icon,ul.checks li.bad,td.bad,span.bad,.error{color:var(--bad)}li.skip{color:var(--muted)}
.status{padding:0 6px;border-radius:4px;background:var(--ok-soft);color:var(--ok)}.bad .status{background:var(--bad-soft);color:var(--bad)}.skip .status{background:var(--bg);color:var(--muted)}
.url{margin-left:1.6em;overflow-wrap:anywhere}ul.checks{margin:4px 0 0 1.6em}ul.checks li{font-size:14px}
.error{margin-left:1.6em;color:var(--bad)}.warn{margin-left:1.6em;color:var(--warn);font-size:14px}
details{margin-left:1.6em}pre{white-space:pre-wrap;background:var(--bg);padding:8px;border-radius:6px}
ul.thresholds li{display:flex;gap:10px;padding:2px 0}ul.thresholds .num{margin-left:auto}
table{width:100%;border-collapse:collapse}th{font-weight:500;color:var(--muted);font-size:12px;text-align:right;padding:4px 8px}th:first-child,td:first-child{text-align:left;font-family:inherit;font-size:14px}
td{text-align:right;padding:6px 8px;border-top:1px solid var(--line);white-space:nowrap}
svg{width:100%;height:160px;display:block}svg .req{fill:var(--accent);opacity:.55}svg .fail{fill:var(--bad)}svg .p95{fill:none;stroke:var(--warn);stroke-width:2;vector-effect:non-scaling-stroke}
.legend{display:flex;gap:16px;font-size:12px;color:var(--muted);margin-bottom:6px}.sw{display:inline-block;width:10px;height:10px;border-radius:2px;margin-right:4px;vertical-align:-1px}
.sw.req{background:var(--accent);opacity:.55}.sw.fail{background:var(--bad)}.sw.p95{height:2px;background:var(--warn);vertical-align:3px}
.trace{user-select:all}
.axis{display:flex;justify-content:space-between;color:var(--muted);font-size:11px}
footer{color:var(--muted);font-size:12px;text-align:center;margin-top:24px}
"#;

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn ms(v: f64) -> String {
    if v >= 1000.0 {
        format!("{:.2} s", v / 1000.0)
    } else if v >= 100.0 {
        format!("{v:.0} ms")
    } else {
        format!("{v:.1} ms")
    }
}

fn bytes(b: f64) -> String {
    if b >= 1_048_576.0 {
        format!("{:.1} MB", b / 1_048_576.0)
    } else if b >= 1024.0 {
        format!("{:.1} KB", b / 1024.0)
    } else {
        format!("{b:.0} B")
    }
}

#[cfg(test)]
mod tests;
