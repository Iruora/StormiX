//! JUnit XML, which every CI system can show as a test report.

use std::fmt::Write;

use crate::FlowResult;

/// One `<testsuite>` for the flow and one `<testcase>` per step. A failed
/// check or a missing response is a `<failure>`; a step not run because of
/// `bail` is `<skipped>`.
pub fn junit_report(result: &FlowResult) -> String {
    let failures = result.failed_steps();
    let skipped = result.steps.iter().filter(|s| s.skipped).count();
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        xml,
        r#"<testsuites name="stormix" tests="{}" failures="{failures}" skipped="{skipped}" time="{:.3}">"#,
        result.steps.len(),
        result.duration_ms / 1000.0,
    );
    let _ = writeln!(
        xml,
        r#"  <testsuite name="{}" tests="{}" failures="{failures}" skipped="{skipped}" time="{:.3}">"#,
        escape(&result.name),
        result.steps.len(),
        result.duration_ms / 1000.0,
    );
    for step in &result.steps {
        let _ = write!(
            xml,
            r#"    <testcase classname="{}" name="{}" time="{:.3}""#,
            escape(&result.name),
            escape(&step.name),
            step.duration_ms / 1000.0,
        );
        if step.passed() {
            xml.push_str("/>\n");
            continue;
        }
        xml.push_str(">\n");
        if step.skipped {
            xml.push_str("      <skipped message=\"an earlier step failed\"/>\n");
        } else if let Some(error) = &step.error {
            let _ = writeln!(
                xml,
                r#"      <failure message="{}" type="request">{} {}</failure>"#,
                escape(error),
                escape(&step.method),
                escape(&step.url),
            );
        } else {
            let failed: Vec<_> = step.checks.iter().filter(|c| !c.passed).collect();
            let details: Vec<String> = failed
                .iter()
                .map(|c| match &c.message {
                    Some(m) => format!("{}: {m}", c.name),
                    None => c.name.clone(),
                })
                .collect();
            let _ = writeln!(
                xml,
                r#"      <failure message="{} of {} checks failed" type="assertion">{}</failure>"#,
                failed.len(),
                step.checks.len(),
                escape(&details.join("\n")),
            );
        }
        xml.push_str("    </testcase>\n");
    }
    xml.push_str("  </testsuite>\n</testsuites>\n");
    xml
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#10;"),
            // Control characters other than tab are not allowed in XML 1.0.
            c if c.is_control() && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Check, StepResult};
    use std::collections::BTreeMap;

    fn step(name: &str, checks: Vec<Check>, error: Option<&str>, skipped: bool) -> StepResult {
        StepResult {
            name: name.into(),
            method: "GET".into(),
            url: "https://x.test/?a=1&b=2".into(),
            status: error.is_none().then_some(200),
            duration_ms: 250.0,
            time_to_headers_ms: None,
            response_bytes: 0,
            checks,
            extracted: BTreeMap::new(),
            warnings: Vec::new(),
            logs: Vec::new(),
            error: error.map(Into::into),
            skipped,
        }
    }

    #[test]
    fn reports_passes_failures_errors_and_skips() {
        let ok = Check {
            name: "status is 200".into(),
            passed: true,
            message: None,
        };
        let bad = Check {
            name: "$.user equals \"demo\"".into(),
            passed: false,
            message: Some("got \"x\"".into()),
        };
        let result = FlowResult {
            name: "Login <smoke>".into(),
            steps: vec![
                step("ok", vec![ok.clone()], None, false),
                step("bad", vec![ok, bad], None, false),
                step("down", vec![], Some("connection refused"), false),
                step("later", vec![], None, true),
            ],
            duration_ms: 1500.0,
        };
        let xml = junit_report(&result);
        assert!(xml.contains(r#"<testsuite name="Login &lt;smoke&gt;" tests="4" failures="2" skipped="1" time="1.500">"#), "{xml}");
        assert!(
            xml.contains(r#"<testcase classname="Login &lt;smoke&gt;" name="ok" time="0.250"/>"#)
        );
        assert!(xml.contains(r#"<failure message="1 of 2 checks failed" type="assertion">$.user equals &quot;demo&quot;: got &quot;x&quot;</failure>"#));
        assert!(xml.contains(r#"<failure message="connection refused" type="request">GET https://x.test/?a=1&amp;b=2</failure>"#));
        assert!(xml.contains("<skipped message=\"an earlier step failed\"/>"));
    }
}
