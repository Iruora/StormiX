import { Waypoints } from "lucide-react";

import type { TraceMode } from "../bindings";

const OPTIONS: { value: TraceMode | null; label: string; title: string }[] = [
  { value: null, label: "Off", title: "Send no traceparent header" },
  {
    value: "request",
    label: "Each request",
    title: "Every request starts a trace of its own: one trace ID per request",
  },
  {
    value: "flow",
    label: "Each flow run",
    title: "Every run of the flow (each load test iteration) is one trace, with a span per request",
  },
];

/**
 * Turns the flow's `trace` setting on or off: a W3C `traceparent` header on
 * every request, so Dynatrace and other APM tools can follow it through the
 * services it reaches, and Stormix records the trace ID to look up.
 */
export function TraceSetting({
  value,
  onChange,
  compact = false,
}: {
  value: TraceMode | null;
  onChange: (value: TraceMode | null) => void;
  compact?: boolean;
}) {
  return (
    <div className={compact ? "" : "rounded-lg border border-line bg-surface p-3"}>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <span className="flex items-center gap-1.5 text-xs font-medium text-subtle">
          <Waypoints size="0.8125rem" />
          Trace requests
        </span>
        <div role="radiogroup" aria-label="Trace requests" className="flex rounded-md border border-line p-0.5">
          {OPTIONS.map((o) => {
            const on = (value ?? null) === o.value;
            return (
              <button
                key={o.label}
                type="button"
                role="radio"
                aria-checked={on}
                title={o.title}
                onClick={() => onChange(o.value)}
                className={`rounded px-2 py-0.5 text-xs ${
                  on ? "bg-accent-soft font-medium text-accent" : "text-muted hover:bg-surface-2 hover:text-ink"
                }`}
              >
                {o.label}
              </button>
            );
          })}
        </div>
      </div>
      <p className="mt-1.5 text-xs text-subtle">
        {value
          ? "Each request carries a W3C traceparent header, and its trace ID is shown in Details and the results. Search for it in Dynatrace or another APM tool to see the request in every service it reached. A step's own traceparent header is kept."
          : "Off: no traceparent header is added. Turn it on to find a slow or failed request in Dynatrace or another APM tool by its trace ID."}
      </p>
    </div>
  );
}
