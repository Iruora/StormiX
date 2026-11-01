//! W3C Trace Context (`traceparent`), so APM tools can follow a request
//! through the services it reaches and Stormix can say which trace it was.
//!
//! <https://www.w3.org/TR/trace-context/#traceparent-header>

/// The header, lower case as HTTP/2 sends it.
pub(crate) const HEADER: &str = "traceparent";

/// A trace ID and the ID of the span a request is sent from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TraceParent {
    /// 32 lower-case hex digits, not all zero.
    pub trace_id: String,
    /// 16 lower-case hex digits, not all zero.
    pub span_id: String,
}

impl TraceParent {
    /// A span in a trace of its own.
    pub fn new() -> Self {
        Self::child_of(&new_trace_id())
    }

    /// A new span in the trace `trace_id`.
    pub fn child_of(trace_id: &str) -> Self {
        Self {
            trace_id: trace_id.to_owned(),
            span_id: format!("{:016x}", fastrand::u64(1..)),
        }
    }

    /// Version 00, sampled, so the services downstream record it.
    pub fn header_value(&self) -> String {
        format!("00-{}-{}-01", self.trace_id, self.span_id)
    }

    /// Reads a `traceparent` value a step set itself. `None` when it is not
    /// valid, since tracing tools ignore those too.
    pub fn parse(value: &str) -> Option<Self> {
        let parts: Vec<&str> = value.trim().split('-').collect();
        let [version, trace_id, span_id, flags, ..] = parts[..] else {
            return None;
        };
        let hex = |s: &str, len: usize| {
            s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        };
        let zero = |s: &str| s.bytes().all(|b| b == b'0');
        // Version 00 has exactly four parts; later versions may add more.
        let valid = hex(version, 2)
            && version != "ff"
            && (version != "00" || parts.len() == 4)
            && hex(trace_id, 32)
            && !zero(trace_id)
            && hex(span_id, 16)
            && !zero(span_id)
            && hex(flags, 2);
        valid.then(|| Self {
            trace_id: trace_id.to_owned(),
            span_id: span_id.to_owned(),
        })
    }
}

/// 16 random bytes as hex. A v4 UUID has 122 random bits, never all zero.
pub(crate) fn new_trace_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_values_are_valid_traceparents() {
        let a = TraceParent::new();
        let b = TraceParent::new();
        assert_ne!(a.trace_id, b.trace_id);
        let header = a.header_value();
        assert_eq!(header.len(), 55, "{header}");
        assert_eq!(TraceParent::parse(&header), Some(a.clone()));
        let child = TraceParent::child_of(&a.trace_id);
        assert_eq!(child.trace_id, a.trace_id);
        assert_ne!(child.span_id, a.span_id);
    }

    #[test]
    fn reads_the_spec_example() {
        let parsed =
            TraceParent::parse("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01").unwrap();
        assert_eq!(parsed.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(parsed.span_id, "00f067aa0ba902b7");
    }

    #[test]
    fn refuses_invalid_values() {
        for bad in [
            "",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7",
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01",
            "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
            "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-extra",
            "{{trace}}",
        ] {
            assert_eq!(TraceParent::parse(bad), None, "{bad}");
        }
        // A later version may carry more fields.
        assert!(
            TraceParent::parse("01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-x")
                .is_some()
        );
    }
}
