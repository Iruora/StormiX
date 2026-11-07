import { Cpu, Plus, X } from "lucide-react";

import type { Monitor, MonitorTarget } from "../bindings";

export type MonitorKind = "prometheus" | "actuator" | "dynatrace";

/** Which kind of endpoint a `monitor` entry reads. */
export function monitorKind(m: Monitor): MonitorKind {
  if ("actuator" in m && m.actuator) return "actuator";
  if ("dynatrace" in m && m.dynatrace) return "dynatrace";
  return "prometheus";
}

/** The URL, name, headers and health check of a `monitor` entry, whatever its kind. */
export function monitorTarget(m: Monitor): MonitorTarget {
  return (m as unknown as Record<MonitorKind, MonitorTarget>)[monitorKind(m)];
}

/** A `monitor` entry of the given kind around the target. */
function make(kind: MonitorKind, target: MonitorTarget): Monitor {
  return { [kind]: target } as unknown as Monitor;
}

const PLACEHOLDER: Record<MonitorKind, string> = {
  prometheus: "{{base_url}}/actuator/prometheus",
  actuator: "{{base_url}}/actuator",
  dynatrace: "https://abc12345.live.dynatrace.com",
};

/** The time between reads when the entry does not say. */
const EVERY: Record<MonitorKind, string> = { prometheus: "1s", actuator: "1s", dynatrace: "30s" };

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
          {value.length === 0 ? "Not watched" : `Read every ${monitorTarget(value[0]).interval ?? EVERY[monitorKind(value[0])]} during the test`}
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
                  <option value="dynatrace">Dynatrace</option>
                </select>
                <input
                  aria-label={kind === "dynatrace" ? "Dynatrace environment URL" : "Metrics URL"}
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
                {kind === "dynatrace" && (
                  <>
                    <input
                      aria-label="Entity selector"
                      value={target.entity ?? ""}
                      placeholder={'entity: type("PROCESS_GROUP_INSTANCE"),entityName.contains("orders")'}
                      title="Which processes are the API, as a Dynatrace entity selector. Their CPU and memory are summed."
                      onChange={(e) => {
                        const { entity: _old, ...rest } = target;
                        set(i, make(kind, e.target.value ? { ...rest, entity: e.target.value } : rest));
                      }}
                      className="mono min-w-[14rem] flex-[3] rounded-md border border-line bg-surface px-2 py-1 text-xs"
                    />
                    <input
                      aria-label="API token"
                      value={target.token ?? ""}
                      placeholder="token: {{dynatrace_token}}"
                      title="An API token with the metrics.read scope. Use a {{variable}} from a secret so the token stays out of the file."
                      onChange={(e) => {
                        const { token: _old, ...rest } = target;
                        set(i, make(kind, e.target.value ? { ...rest, token: e.target.value } : rest));
                      }}
                      className="mono min-w-[9rem] flex-1 rounded-md border border-line bg-surface px-2 py-1 text-xs"
                    />
                  </>
                )}
                <input
                  aria-label="Health check URL"
                  value={target.health ?? ""}
                  placeholder={kind === "actuator" ? "health: …/actuator/health (automatic)" : "health URL (optional)"}
                  title="Read with the metrics to spot the API going down. Actuator reads /health under its URL when this is empty."
                  onChange={(e) => {
                    const { health: _old, ...rest } = target;
                    set(i, make(kind, e.target.value ? { ...rest, health: e.target.value } : rest));
                  }}
                  className="mono min-w-[10rem] flex-[2] rounded-md border border-line bg-surface px-2 py-1 text-xs"
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
        <span className="mono">{"{{variables}}"}</span>. A health check (like{" "}
        <span className="mono">/actuator/health</span> or <span className="mono">/healthz</span>) is read at the same
        time to spot the API going down; Actuator reads its own <span className="mono">/health</span>. Dynatrace
        takes the environment URL, an entity selector for the API&apos;s processes and a token with{" "}
        <span className="mono">metrics.read</span>; it has one reading per minute, a minute or two late.
      </p>
    </div>
  );
}
