import { Check, CircleCheck, CircleX, Copy, X } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import type { HeaderPair, Payload, StepResult } from "../bindings";

type Tab = "failure" | "request" | "response";

/**
 * Everything about one step: why it failed, the request as sent and the
 * response as received. Opens over the app; Escape or the backdrop closes it.
 */
export function StepDetails({ step, onClose }: { step: StepResult; onClose: () => void }) {
  const exchange = step.exchange ?? null;
  const response = exchange?.response ?? null;
  const failedChecks = step.checks.filter((c) => !c.passed);
  const passed = !step.skipped && step.error == null && failedChecks.length === 0 && (step.status ?? 0) < 400;
  const [tab, setTab] = useState<Tab>(passed ? "response" : "failure");

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={`Details of ${step.name}`}
        className="flex max-h-full w-full max-w-3xl flex-col overflow-hidden rounded-xl border border-line bg-surface shadow-2xl"
      >
        <header className="flex items-start gap-3 border-b border-line px-5 py-3">
          {passed ? (
            <CircleCheck size={18} className="mt-0.5 shrink-0 text-ok" />
          ) : (
            <CircleX size={18} className="mt-0.5 shrink-0 text-bad" />
          )}
          <div className="min-w-0 flex-1">
            <div className="font-semibold">{step.name}</div>
            <div className="mono truncate text-xs text-muted" title={step.url}>
              <span className="font-semibold text-accent">{step.method}</span> {step.url}
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-2 text-xs text-muted">
            {step.status != null && (
              <span className={`mono rounded px-1.5 py-px font-semibold ${statusTone(step.status)}`}>
                {step.status} {response?.statusText}
              </span>
            )}
            <span className="mono">{formatMs(step.durationMs ?? 0)}</span>
            {exchange && <CopyButton label="Copy as cURL" text={() => curl(step)} />}
            <button
              type="button"
              aria-label="Close"
              onClick={onClose}
              className="rounded p-1 text-muted hover:bg-surface-2 hover:text-ink"
            >
              <X size={16} />
            </button>
          </div>
        </header>

        <nav className="flex gap-1 border-b border-line bg-surface-2 px-4 py-1.5">
          {(["failure", "request", "response"] as const).map((t) => (
            <button
              key={t}
              type="button"
              onClick={() => setTab(t)}
              className={`rounded px-2.5 py-0.5 text-xs font-medium ${
                tab === t ? "bg-accent-soft text-accent" : "text-muted hover:bg-surface"
              }`}
            >
              {t === "failure" ? (passed ? "Checks" : "Failure") : t === "request" ? "Request" : "Response"}
            </button>
          ))}
        </nav>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          {tab === "failure" && (
            <>
              <Outcome step={step} />
              {!passed && response && (
                <Section title="Response body">
                  <Body payload={response.body ?? null} empty="The response had no body." />
                </Section>
              )}
            </>
          )}
          {tab === "request" &&
            (exchange ? (
              <>
                <Section title="Headers">
                  <Headers headers={exchange.requestHeaders} />
                </Section>
                <Section title="Body">
                  <Body payload={exchange.requestBody ?? null} empty="No body was sent." />
                </Section>
              </>
            ) : (
              <NotKept />
            ))}
          {tab === "response" &&
            (!exchange ? (
              <NotKept />
            ) : !response ? (
              <p className="text-[13px] text-muted">
                No response came back{step.error ? `: ${step.error}` : "."}
              </p>
            ) : (
              <>
                <Section title="Headers">
                  <Headers headers={response.headers} />
                </Section>
                <Section title="Body">
                  <Body payload={response.body ?? null} empty="The response had no body." />
                </Section>
              </>
            ))}
        </div>
      </div>
    </div>
  );
}

/** Why the step failed first, then everything else it reported. */
function Outcome({ step }: { step: StepResult }) {
  const failed = step.checks.filter((c) => !c.passed);
  const passed = step.checks.filter((c) => c.passed);
  const extracted = Object.entries(step.extracted);
  const unchecked = step.error == null && failed.length === 0 && (step.status ?? 0) >= 400;
  return (
    <>
      {step.error && (
        <Section title="Error">
          <div className="rounded-md border border-bad/30 bg-bad-soft px-3 py-2 text-[13px] text-bad">{step.error}</div>
        </Section>
      )}
      {unchecked && (
        <Section title="Error">
          <div className="rounded-md border border-bad/30 bg-bad-soft px-3 py-2 text-[13px] text-bad">
            Status {step.status}. Under load a 4xx or 5xx counts as a failed request even without an assertion.
          </div>
        </Section>
      )}
      {failed.length > 0 && (
        <Section title={`Failed checks (${failed.length})`}>
          <ul className="space-y-1.5">
            {failed.map((c, i) => (
              <li key={i} className="rounded-md border border-bad/30 bg-bad-soft px-3 py-2 text-[13px]">
                <div className="mono font-medium text-bad">{c.name}</div>
                {c.message && <div className="mt-0.5 text-ink">{c.message}</div>}
              </li>
            ))}
          </ul>
        </Section>
      )}
      {step.warnings.length > 0 && (
        <Section title="Warnings">
          <ul className="space-y-1 text-[13px] text-warn">
            {step.warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </Section>
      )}
      {passed.length > 0 && (
        <Section title={`Passed checks (${passed.length})`}>
          <ul className="space-y-0.5 text-[13px]">
            {passed.map((c, i) => (
              <li key={i} className="flex items-center gap-1.5 text-muted">
                <Check size={13} className="text-ok" />
                <span className="mono">{c.name}</span>
              </li>
            ))}
          </ul>
        </Section>
      )}
      {extracted.length > 0 && (
        <Section title="Variables set">
          <Table rows={extracted.map(([name, value]) => ({ name, value }))} />
        </Section>
      )}
      {step.logs.length > 0 && (
        <Section title="Console">
          <Pre>{step.logs.join("\n")}</Pre>
        </Section>
      )}
      {step.checks.length === 0 && !step.error && !unchecked && (
        <p className="text-[13px] text-muted">This step has no checks.</p>
      )}
    </>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="mb-4 last:mb-0">
      <h3 className="mb-1.5 text-xs font-semibold tracking-wide text-subtle uppercase">{title}</h3>
      {children}
    </section>
  );
}

function Headers({ headers }: { headers: HeaderPair[] }) {
  if (headers.length === 0) return <p className="text-[13px] text-muted">None.</p>;
  return <Table rows={headers} />;
}

function Table({ rows }: { rows: { name: string; value: string }[] }) {
  return (
    <table className="mono w-full text-xs">
      <tbody>
        {rows.map((h, i) => (
          <tr key={i} className="border-t border-line first:border-t-0">
            <td className="w-1/3 py-1 pr-3 align-top text-muted">{h.name}</td>
            <td className="py-1 break-all">{h.value}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Body({ payload, empty }: { payload: Payload | null; empty: string }) {
  if (!payload) return <p className="text-[13px] text-muted">{empty}</p>;
  if (payload.binary) return <p className="text-[13px] text-muted">{payload.text}</p>;
  const text = pretty(payload.text);
  return (
    <>
      <div className="relative">
        <Pre>{text}</Pre>
        <div className="absolute top-1.5 right-1.5">
          <CopyButton label="Copy" text={() => payload.text} />
        </div>
      </div>
      <div className="mt-1 text-xs text-subtle">
        {formatBytes(payload.bytes)}
        {payload.truncated && `, cut to the first ${formatBytes(new TextEncoder().encode(payload.text).length)}`}
      </div>
    </>
  );
}

function Pre({ children }: { children: ReactNode }) {
  return (
    <pre className="mono max-h-80 overflow-auto rounded-md bg-surface-2 px-3 py-2 text-xs leading-5 whitespace-pre-wrap break-all">
      {children}
    </pre>
  );
}

function NotKept() {
  return (
    <p className="text-[13px] text-muted">
      Not kept for this step. Runs saved before Stormix 1.1 have no request or response, and load tests keep them only
      for failures.
    </p>
  );
}

function CopyButton({ label, text }: { label: string; text: () => string }) {
  const [done, setDone] = useState(false);
  return (
    <button
      type="button"
      onClick={async () => {
        await navigator.clipboard.writeText(text());
        setDone(true);
        setTimeout(() => setDone(false), 1200);
      }}
      className="flex items-center gap-1 rounded border border-line bg-surface px-1.5 py-0.5 text-xs text-muted hover:text-ink"
    >
      {done ? <Check size={12} className="text-ok" /> : <Copy size={12} />}
      {done ? "Copied" : label}
    </button>
  );
}

/** JSON is indented; anything else is shown as it came. */
function pretty(text: string): string {
  const t = text.trim();
  if (!(t.startsWith("{") || t.startsWith("["))) return text;
  try {
    return JSON.stringify(JSON.parse(t), null, 2);
  } catch {
    return text;
  }
}

/** The request as a shell command, to replay it outside Stormix. */
export function curl(step: StepResult): string {
  const quote = (s: string) => `'${s.replace(/'/g, `'\\''`)}'`;
  const parts = [`curl -X ${step.method} ${quote(step.url)}`];
  for (const h of step.exchange?.requestHeaders ?? []) {
    if (h.name === "content-length" || (h.name === "content-type" && h.value.startsWith("multipart/"))) continue;
    parts.push(`-H ${quote(`${h.name}: ${h.value}`)}`);
  }
  const headers = step.exchange?.requestHeaders ?? [];
  const multipart = headers.some((h) => h.name === "content-type" && h.value.startsWith("multipart/"));
  const body = step.exchange?.requestBody;
  if (body && !body.binary) {
    // Multipart fields are kept one `key=value` per line; curl builds the boundary itself.
    if (multipart) parts.push(...body.text.split("\n").map((field) => `-F ${quote(field)}`));
    else parts.push(`--data-raw ${quote(body.text)}`);
  }
  return parts.join(" \\\n  ");
}

function statusTone(status: number): string {
  return status < 300 ? "bg-ok-soft text-ok" : status < 400 ? "bg-surface-2 text-muted" : "bg-bad-soft text-bad";
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

function formatMs(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${Math.round(ms)} ms`;
}
