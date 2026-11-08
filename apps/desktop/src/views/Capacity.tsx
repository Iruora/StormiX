import { CircleCheck, CircleX, Server } from "lucide-react";
import { useState } from "react";

import type { Capacity, LimitSearch, LimitStep } from "../bindings";
import { capacityInput, sizePods, type CapacityInput } from "../lib/sizing";

const input =
  "rounded-md border border-line bg-surface px-2 py-1 text-[0.8125rem] outline-none focus:border-accent";

/** A number box; empty means "not set" where `optional`. */
function NumField({
  label,
  title,
  value,
  optional,
  suffix,
  onChange,
}: {
  label: string;
  title?: string;
  value: number | null;
  optional?: boolean;
  suffix?: string;
  onChange: (value: number | null) => void;
}) {
  const [text, setText] = useState(value == null ? "" : String(value));
  return (
    <label className="flex flex-col gap-0.5 text-subtle" title={title}>
      {label}
      <span className="flex items-center gap-1">
        <input
          aria-label={label}
          inputMode="decimal"
          value={text}
          placeholder={optional ? "–" : undefined}
          onChange={(e) => {
            const next = e.target.value;
            if (!/^\d*\.?\d*$/.test(next)) return;
            setText(next);
            if (next === "") {
              if (optional) onChange(null);
            } else onChange(Number(next));
          }}
          className={`${input} mono w-20 text-xs text-ink`}
        />
        {suffix && <span className="text-xs">{suffix}</span>}
      </span>
    </label>
  );
}

/** The inputs to size pods from: production peak, growth, CPU target, pod counts. */
function CapacityFields({ value, onChange }: { value: CapacityInput; onChange: (next: CapacityInput) => void }) {
  const set = <K extends keyof CapacityInput>(key: K, v: CapacityInput[K]) => onChange({ ...value, [key]: v });
  return (
    <div className="flex flex-wrap items-end gap-3 text-xs">
      <NumField label="peak req/s" title="Requests per second at the busiest time in production, all pods together" value={value.peak_rps} onChange={(v) => set("peak_rps", v ?? 0)} />
      <NumField label="growth" suffix="%" title="Traffic growth to plan for" value={value.growth} onChange={(v) => set("growth", v ?? 0)} />
      <NumField label="target CPU" suffix="%" title="CPU each pod should run at during the peak, leaving room for spikes and a pod restarting" value={value.target_cpu} onChange={(v) => set("target_cpu", v ?? 0)} />
      <NumField label="pods in the test" title="Pods behind the URL during the test; the limit is divided by it" value={value.tested_pods} onChange={(v) => set("tested_pods", Math.max(1, Math.round(v ?? 1)))} />
      <NumField label="spare pods" title="Extra pods so one can fail or be redeployed" value={value.spare_pods} onChange={(v) => set("spare_pods", Math.round(v ?? 0))} />
      <NumField label="running now" optional title="Pods in production today, to compare with" value={value.current_pods} onChange={(v) => set("current_pods", v == null ? null : Math.round(v))} />
      <NumField label="cores per pod" optional title="The pod's CPU limit in cores. Left empty: what the monitor reports, else 1" value={value.pod_cores} onChange={(v) => set("pod_cores", v)} />
    </div>
  );
}

/**
 * The load setup's `capacity` section, shown for a find_limit profile: what
 * the result sizes pods for. Saved into the test file.
 */
export function CapacitySetting({ value, onChange }: { value: Capacity | null; onChange: (next: Capacity | null) => void }) {
  return (
    <section className="rounded-lg border border-line bg-surface">
      <div className="flex items-center gap-2 border-b border-line px-3 py-2">
        <Server size="0.875rem" className="text-subtle" />
        <span className="font-medium">Size pods for</span>
        <label className="ml-auto flex items-center gap-1.5 text-xs text-muted">
          <input
            type="checkbox"
            checked={value != null}
            onChange={(e) => onChange(e.target.checked ? capacityInput(null) : null)}
            className="size-3.5 accent-accent"
          />
          Work out the pods needed
        </label>
      </div>
      <div className="px-3 py-2.5">
        {value ? (
          <CapacityFields value={capacityInput(value)} onChange={(next) => onChange(next)} />
        ) : (
          <span className="text-xs text-subtle">
            Turn on to see how many pods your production peak needs, from the limit this test finds.
          </span>
        )}
      </div>
    </section>
  );
}

function formatMs(ms: number): string {
  if (ms >= 1000) return `${(ms / 1000).toFixed(2)} s`;
  if (ms >= 100) return `${Math.round(ms)} ms`;
  return `${ms.toFixed(1)} ms`;
}

/** A threshold's measured value in its own unit. */
function measuredText(threshold: string, actual: number): string {
  const metric = threshold.split(/[\s<>]/)[0];
  if (metric === "errors" || metric === "checks") return `${actual.toFixed(2)}%`;
  if (metric === "cpu") return `${Math.round(actual)}%`;
  if (metric === "rps") return `${actual.toFixed(1)}/s`;
  return formatMs(actual);
}

const n1 = (v: number) => (v < 10 ? v.toFixed(1) : v.toFixed(0));

/**
 * A find_limit result: each step, the most the API sustained, and a pods
 * calculator that starts from the file's `capacity` section and can be
 * changed to try other peaks without running the test again.
 */
export function LimitResult({ limit, capacity }: { limit: LimitSearch; capacity: Capacity | null | undefined }) {
  const steps = limit.steps as LimitStep[];
  const sustained = limit.sustained != null ? steps[limit.sustained] : undefined;
  const [inputs, setInputs] = useState<CapacityInput>(() => capacityInput(limit.sizing?.capacity ?? capacity));
  const sizing = sizePods(limit, inputs);
  const maxRps = Math.max(1, ...steps.map((s) => s.rps ?? 0));
  const headline = !sustained
    ? "Not found: even the first step broke a threshold. Start lower."
    : limit.reached
      ? `${(sustained.rps ?? 0).toFixed(1)} req/s sustained (${sustained.rate} iterations/s); the next step broke a threshold.`
      : `Not reached: ${(sustained.rps ?? 0).toFixed(1)} req/s held at the max rate, so the API takes at least that. Raise the max rate to find its limit.`;
  const reference = sizing?.cpuStep != null && sizing.cpuStep !== limit.sustained ? steps[sizing.cpuStep] : undefined;
  const why = !sizing
    ? ""
    : sizing.basis === "cpu"
      ? reference
        ? `the CPU was saturated at the top, so this scales the ${reference.rate}/s step (${n1((reference.apiCpu ?? 0) / sizing.podCores)}% CPU) to the ${inputs.target_cpu}% target`
        : `scaled from ${n1(sizing.testedCpu ?? 0)}% CPU to the ${inputs.target_cpu}% target`
      : sizing.basis === "latency"
        ? `the CPU stayed under the ${inputs.target_cpu}% target, so latency or errors set the limit`
        : `no CPU reading (add an API monitor), so the step is taken as saturated and run at ${inputs.target_cpu}%`;
  const current = inputs.current_pods;
  const verdict =
    sizing && current != null
      ? sizing.pods > current
        ? { text: `${current} running now: add ${sizing.pods - current}`, tone: "bad" }
        : sizing.pods < current
          ? { text: `${current} running now: ${current - sizing.pods} more than needed`, tone: "warn" }
          : { text: `${current} running now: just right`, tone: "ok" }
      : null;

  return (
    <div className="border-t border-line px-4 py-3">
      <div className="mb-1 text-xs font-medium text-subtle">Limit</div>
      <p className={`mb-2 text-[0.8125rem] ${sustained ? "text-ink" : "text-bad"}`}>{headline}</p>
      <div className="overflow-x-auto">
        <table className="w-full text-xs">
          <thead className="text-subtle">
            <tr>
              <th className="py-1 pr-2 text-left font-medium">Step</th>
              <th className="py-1 pr-2 text-left font-medium">req/s</th>
              <th className="py-1 pr-2 text-right font-medium">avg</th>
              <th className="py-1 pr-2 text-right font-medium">p95</th>
              <th className="py-1 pr-2 text-right font-medium">errors</th>
              <th className="py-1 pr-2 text-right font-medium">API CPU</th>
            </tr>
          </thead>
          <tbody className="mono whitespace-nowrap">
            {steps.map((s, i) => {
              const held = i === limit.sustained;
              return (
                <tr
                  key={i}
                  title={held ? "The most the API sustained" : s.partial ? "Stopped during this step" : undefined}
                  className={`border-t border-line ${held ? "bg-ok-soft" : ""}`}
                >
                  <td className="whitespace-nowrap py-1 pr-2">
                    <span className="inline-flex items-center gap-1">
                      {s.partial ? (
                        <span className="w-3 text-center text-subtle">…</span>
                      ) : s.passed ? (
                        <CircleCheck size="0.75rem" className="text-ok" />
                      ) : (
                        <CircleX size="0.75rem" className="text-bad" />
                      )}
                      {s.rate}/s
                    </span>
                  </td>
                  <td className="py-1 pr-2">
                    <span className="flex min-w-24 items-center gap-1.5">
                      <span className="w-10 shrink-0 text-right">{(s.rps ?? 0).toFixed(1)}</span>
                      <span
                        className={`h-1.5 rounded-sm ${s.passed ? "bg-accent/60" : s.partial ? "bg-line" : "bg-bad"}`}
                        style={{ width: `${((s.rps ?? 0) / maxRps) * 100}%` }}
                      />
                    </span>
                  </td>
                  <td className="py-1 pr-2 text-right">{formatMs(s.latency.avg ?? 0)}</td>
                  <td className="py-1 pr-2 text-right">{formatMs(s.latency.p95 ?? 0)}</td>
                  <td className={`py-1 pr-2 text-right ${s.failedRequests > 0 ? "text-bad" : ""}`}>{(s.errorRate ?? 0).toFixed(2)}%</td>
                  <td className="py-1 pr-2 text-right">{s.apiCpu == null ? "–" : `${Math.round(s.apiCpu)}%`}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {steps
        .filter((s) => !s.passed && !s.partial)
        .map((s, i) => (
          <p key={i} className="mt-1.5 text-xs text-bad">
            {s.rate}/s broke{" "}
            {s.thresholds
              .filter((t) => !t.passed)
              .map((t) => `${t.threshold} (${t.note ?? measuredText(t.threshold, t.actual ?? 0)})`)
              .join(", ")}
            .
          </p>
        ))}

      {sustained && (
        <div className="mt-3 rounded-lg border border-line">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-line px-3 py-2">
            <Server size="0.875rem" className="text-accent" />
            <span className="font-medium">Pods needed</span>
            {sizing && <span className="text-lg font-semibold tabular-nums">{sizing.pods}</span>}
            {verdict && (
              <span
                className={`rounded px-1.5 py-0.5 text-xs font-medium ${
                  verdict.tone === "bad" ? "bg-bad-soft text-bad" : verdict.tone === "warn" ? "bg-warn-soft text-warn" : "bg-ok-soft text-ok"
                }`}
              >
                {verdict.text}
              </span>
            )}
          </div>
          <div className="border-b border-line px-3 py-2.5">
            <CapacityFields value={inputs} onChange={setInputs} />
          </div>
          {sizing && (
            <ul className="space-y-1 px-3 py-2.5 text-[0.8125rem]">
              <li>
                One pod held <b className="tabular-nums">{sizing.testedRpsPerPod.toFixed(1)} req/s</b>
                {sizing.testedCpu != null && ` at ${Math.round(sizing.testedCpu)}% CPU of ${sizing.podCores} core${sizing.podCores === 1 ? "" : "s"}`}.
              </li>
              <li>
                Usable per pod: <b className="tabular-nums">{sizing.usableRpsPerPod.toFixed(1)} req/s</b>
                <span className="text-muted"> ({why})</span>
              </li>
              <li className="mono text-xs text-muted">
                ⌈{inputs.peak_rps} × {(1 + inputs.growth / 100).toFixed(2)} ÷ {sizing.usableRpsPerPod.toFixed(1)}⌉ ={" "}
                {sizing.podsForTraffic} + {inputs.spare_pods} spare = <b className="text-ink">{sizing.pods} pods</b>
              </li>
              <li className="text-muted">
                In flight: about {n1(sizing.inFlightPerPod)} requests per pod, {n1(sizing.inFlightTotal)} in all (rate ×{" "}
                {formatMs(sizing.latencyMs)} average latency, Little&apos;s law). Size thread and connection pools above
                that.
              </li>
            </ul>
          )}
        </div>
      )}
    </div>
  );
}

/** The find_limit steps finished so far, while the test runs. */
export function LiveLimitSteps({ steps }: { steps: LimitStep[] }) {
  if (!steps.length) return null;
  return (
    <div className="flex flex-wrap items-center gap-1.5 border-t border-line px-4 py-2 text-xs">
      <span className="text-subtle">Steps</span>
      {steps.map((s, i) => (
        <span
          key={i}
          title={s.passed ? "Every threshold held" : `Broke ${s.thresholds.filter((t) => !t.passed).map((t) => t.threshold).join(", ")}`}
          className={`mono inline-flex items-center gap-1 rounded px-1.5 py-0.5 ${s.passed ? "bg-ok-soft text-ok" : "bg-bad-soft text-bad"}`}
        >
          {s.passed ? <CircleCheck size="0.6875rem" /> : <CircleX size="0.6875rem" />}
          {s.rate}/s · p95 {formatMs(s.latency.p95 ?? 0)}
          {s.apiCpu != null && ` · ${Math.round(s.apiCpu)}%`}
        </span>
      ))}
    </div>
  );
}
