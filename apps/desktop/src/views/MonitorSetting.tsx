import { Cpu, Plus, X } from "lucide-react";

import type { Monitor, MonitorTarget } from "../bindings";

export type MonitorKind = "prometheus" | "actuator";

export function monitorKind(m: Monitor): MonitorKind {
  return "actuator" in m && m.actuator ? "actuator" : "prometheus";
}

export function monitorTarget(m: Monitor): MonitorTarget {
  return ("actuator" in m && m.actuator ? m.actuator : (m as { prometheus: MonitorTarget }).prometheus) as MonitorTarget;
}

function make(kind: MonitorKind, target: MonitorTarget): Monitor {
  return (kind === "actuator" ? { actuator: target } : { prometheus: target }) as Monitor;
}

const PLACEHOLDER: Record<MonitorKind, string> = {
  prometheus: "{{base_url}}/actuator/prometheus",
  actuator: "{{base_url}}/actuator",
};

/**
 * The file's `monitor` list: metrics endpoints the API serves, read during
 * the load test so its CPU and memory show beside the latency. Headers, like
 * a token, are kept as they are and edited in the YAML.
 */
export function MonitorSetting({ value, onChange }: { value: Monitor[]; onChange: (next: Monitor[]) => void }) {
  const set = (i: number, next: Monitor | null) =>
    onChange(next ? value.map((m, j) => (j === i ? next : m)) : value.filter((_, j) => j !== i));
  return (
    <div className="rounded-lg border border-line bg-surface p-3">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <span className="flex items-center gap-1.5 text-xs font-medium text-subtle">
          <Cpu size="0.8125rem" />
          API CPU and memory
        </span>
        <span className="text-xs text-muted">
          {value.length === 0 ? "Not watched" : `Read every ${monitorTarget(value[0]).interval ?? "1s"} during the test`}
        </span>
        <button
          type="button"
          onClick={() => onChange([...value, make("prometheus", { url: "" })])}
          className="ml-auto flex items-center gap-1 rounded-md border border-line px-2 py-0.5 text-xs text-muted hover:border-accent hover:text-accent"
        >
          <Plus size="0.75rem" /> Add a metrics endpoint
        </button>
      </div>
      {value.length > 0 && (
        <ul className="mt-2 space-y-1.5">
          {value.map((m, i) => {
            const kind = monitorKind(m);
            const target = monitorTarget(m);
            const headers = target.headers?.length ?? 0;
            return (
              <li key={i} className="flex flex-wrap items-center gap-1.5">
                <select
                  aria-label="Endpoint type"
                  value={kind}
                  onChange={(e) => set(i, make(e.target.value as MonitorKind, target))}
                  className="rounded-md border border-line bg-surface px-1.5 py-1 text-xs"
                >
                  <option value="prometheus">Prometheus text</option>
                  <option value="actuator">Spring Boot Actuator</option>
                </select>
                <input
                  aria-label="Metrics URL"
                  value={target.url}
                  placeholder={PLACEHOLDER[kind]}
                  onChange={(e) => set(i, make(kind, { ...target, url: e.target.value }))}
                  className="mono min-w-[12rem] flex-[3] rounded-md border border-line bg-surface px-2 py-1 text-xs"
                />
                <input
                  aria-label="Name"
                  value={target.name ?? ""}
                  placeholder="name (optional)"
                  onChange={(e) => {
                    const { name: _old, ...rest } = target;
                    set(i, make(kind, e.target.value ? { ...rest, name: e.target.value } : rest));
                  }}
                  className="min-w-[6rem] flex-1 rounded-md border border-line bg-surface px-2 py-1 text-xs"
                />
                {headers > 0 && (
                  <span className="text-[0.6875rem] text-subtle" title="Edit headers in the YAML">
                    + {headers} header{headers === 1 ? "" : "s"}
                  </span>
                )}
                <button
                  type="button"
                  aria-label="Remove this endpoint"
                  onClick={() => set(i, null)}
                  className="rounded p-1 text-muted hover:bg-surface-2 hover:text-bad"
                >
                  <X size="0.8125rem" />
                </button>
              </li>
            );
          })}
        </ul>
      )}
      <p className="mt-2 text-[0.6875rem] text-subtle">
        Prometheus text works with <span className="mono">/actuator/prometheus</span> (Spring Boot with
        micrometer-registry-prometheus), <span className="mono">/q/metrics</span> (Quarkus) and{" "}
        <span className="mono">/metrics</span> (prometheus-net, prom-client, Go). Spring Boot Actuator takes the{" "}
        <span className="mono">/actuator</span> base URL and also works without the Prometheus registry. URLs can use{" "}
        <span className="mono">{"{{variables}}"}</span>.
      </p>
    </div>
  );
}
