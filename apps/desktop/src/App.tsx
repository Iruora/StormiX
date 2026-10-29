import { useQuery } from "@tanstack/react-query";
import {
  CircleAlert,
  CircleCheck,
  FileCode,
  Folders,
  Gauge,
  History,
  RotateCcw,
  TriangleAlert,
  Zap,
} from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import { commands, type Issue, type TestFile } from "./bindings";
import { describeLoad } from "./lib/describe";

export function App() {
  const info = useQuery({ queryKey: ["app-info"], queryFn: commands.getAppInfo });

  return (
    <div className="flex h-full">
      <nav className="flex w-56 shrink-0 flex-col border-r border-line bg-surface">
        <div className="flex items-center gap-2 px-4 py-4">
          <span className="grid size-7 place-items-center rounded-lg bg-accent text-bolt">
            <Zap size={16} fill="currentColor" />
          </span>
          <span className="text-base font-extrabold tracking-tight">Stormix</span>
        </div>
        <NavItem icon={<FileCode size={16} />} label="Test file" active />
        <NavItem icon={<Folders size={16} />} label="Collections" soon="M2" />
        <NavItem icon={<Gauge size={16} />} label="Load tests" soon="M3" />
        <NavItem icon={<History size={16} />} label="Runs" soon="M4" />
        <div className="mt-auto px-4 py-3 text-xs text-subtle">
          {info.data ? `v${info.data.version}` : ""}
        </div>
      </nav>
      <main className="min-w-0 flex-1">
        <TestFileView />
      </main>
    </div>
  );
}

function NavItem({
  icon,
  label,
  active,
  soon,
}: {
  icon: ReactNode;
  label: string;
  active?: boolean;
  soon?: string;
}) {
  return (
    <div
      className={`mx-2 flex items-center gap-2 rounded-md px-2 py-1.5 ${
        active ? "bg-accent-soft font-medium text-accent" : "text-subtle"
      }`}
      title={soon ? `Coming in ${soon}` : undefined}
    >
      {icon}
      <span>{label}</span>
      {soon && (
        <span className="ml-auto rounded bg-surface-2 px-1.5 text-[11px] text-subtle">{soon}</span>
      )}
    </div>
  );
}

/** Edit a test file and see it parsed and checked as you type. */
function TestFileView() {
  const example = useQuery({ queryKey: ["example"], queryFn: commands.exampleTestFile });
  const [text, setText] = useState<string | null>(null);
  const source = text ?? example.data ?? "";
  const debounced = useDebounced(source, 250);

  const validation = useQuery({
    queryKey: ["validate", debounced],
    queryFn: () => commands.validateTestFile(debounced),
    enabled: debounced.length > 0,
    placeholderData: (previous) => previous,
  });
  const result = validation.data;

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-line bg-surface px-5 py-3">
        <h1 className="text-[15px] font-semibold">Test file</h1>
        <span className="text-xs text-muted">
          A flow, an optional load profile and thresholds. Run and load arrive in M1 and M3.
        </span>
        <button
          type="button"
          onClick={() => setText(null)}
          className="ml-auto flex items-center gap-1.5 rounded-md border border-line px-2.5 py-1 text-xs text-muted hover:bg-surface-2"
        >
          <RotateCcw size={13} />
          Reset to example
        </button>
      </header>
      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_minmax(320px,0.8fr)]">
        <textarea
          aria-label="Test file YAML"
          spellCheck={false}
          wrap="off"
          value={source}
          onChange={(e) => setText(e.target.value)}
          className="mono resize-none border-r border-line bg-surface p-4 text-[13px] leading-5 outline-none"
        />
        <aside className="min-h-0 overflow-y-auto p-4">
          {result?.parseError ? (
            <Banner tone="bad" icon={<CircleAlert size={16} />} title="The file does not parse">
              <pre className="mono whitespace-pre-wrap text-xs">{result.parseError}</pre>
            </Banner>
          ) : result?.test ? (
            <>
              <Issues issues={result.issues} />
              <Summary test={result.test} />
            </>
          ) : null}
        </aside>
      </div>
    </div>
  );
}

function Issues({ issues }: { issues: Issue[] }) {
  if (issues.length === 0) {
    return (
      <Banner tone="ok" icon={<CircleCheck size={16} />} title="Valid">
        No problems found.
      </Banner>
    );
  }
  const errors = issues.filter((i) => i.severity === "error").length;
  return (
    <Banner
      tone={errors > 0 ? "bad" : "warn"}
      icon={errors > 0 ? <CircleAlert size={16} /> : <TriangleAlert size={16} />}
      title={`${count(errors, "error")}, ${count(issues.length - errors, "warning")}`}
    >
      <ul className="space-y-1.5">
        {issues.map((issue, i) => (
          <li key={i}>
            <span className="mono text-xs opacity-80">{issue.path}</span>
            <div>{issue.message}</div>
          </li>
        ))}
      </ul>
    </Banner>
  );
}

function Summary({ test }: { test: TestFile }) {
  const { flow } = test;
  return (
    <section className="mt-4 rounded-lg border border-line bg-surface">
      <div className="border-b border-line px-4 py-3">
        <div className="font-semibold">{flow.name}</div>
        <div className="text-xs text-muted">{describeLoad(test.load)}</div>
      </div>
      <ol className="divide-y divide-line">
        {flow.steps.map((step, i) => (
          <li key={i} className="flex items-baseline gap-3 px-4 py-2">
            <span className="w-4 text-right text-xs text-subtle">{i + 1}</span>
            <span className="mono w-14 shrink-0 text-xs font-semibold text-accent">
              {step.request.method}
            </span>
            <div className="min-w-0">
              <div>{step.name}</div>
              <div className="mono truncate text-xs text-muted">{step.request.url}</div>
            </div>
          </li>
        ))}
      </ol>
      {test.thresholds && test.thresholds.length > 0 && (
        <div className="flex flex-wrap gap-1.5 border-t border-line px-4 py-3">
          {test.thresholds.map((t) => (
            <span key={t} className="mono rounded bg-surface-2 px-1.5 py-0.5 text-xs">
              {t}
            </span>
          ))}
        </div>
      )}
    </section>
  );
}

const TONES = {
  ok: "border-ok/30 bg-ok-soft text-ok",
  warn: "border-warn/30 bg-warn-soft text-warn",
  bad: "border-bad/30 bg-bad-soft text-bad",
};

function Banner({
  tone,
  icon,
  title,
  children,
}: {
  tone: keyof typeof TONES;
  icon: ReactNode;
  title: string;
  children: ReactNode;
}) {
  return (
    <div className={`rounded-lg border px-3 py-2.5 ${TONES[tone]}`}>
      <div className="flex items-center gap-2 font-semibold">
        {icon}
        {title}
      </div>
      <div className="mt-1.5 text-[13px] text-ink">{children}</div>
    </div>
  );
}

function count(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? "" : "s"}`;
}

function useDebounced<T>(value: T, ms: number): T {
  const [current, setCurrent] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setCurrent(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return current;
}
