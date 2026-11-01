import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  CircleAlert,
  CircleCheck,
  FileCode,
  FileJson,
  FilePlus,
  FileUp,
  FolderOpen,
  Gauge,
  History,
  LoaderCircle,
  PanelRightClose,
  PanelRightOpen,
  Play,
  Save,
  TriangleAlert,
  X,
  Zap,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";

import {
  commands,
  events,
  type FlowResult,
  type ImportedCollection,
  type Issue,
  type LoadProgress,
  type TestFile,
  type TestFile_Deserialize,
} from "./bindings";
import { YamlEditor } from "./editor/YamlEditor";
import { describeLoad } from "./lib/describe";
import { methodText } from "./lib/methods";
import { VariablesContext, variableTable } from "./lib/variables";
import { LoadLive, LoadResult } from "./views/LoadResult";
import { RunResult, stepPassed } from "./views/RunResult";
import { RUNS_KEY, RunsView } from "./views/Runs";
import { SplitView, usePanel } from "./views/SidePanel";
import { EnvSecrets } from "./views/EnvSecrets";
import { FlowView } from "./views/FlowView";
import { StepsEditor } from "./views/StepsEditor";
import { UpdateBanner } from "./views/UpdateBanner";

export function App() {
  const info = useQuery({ queryKey: ["app-info"], queryFn: commands.getAppInfo });
  const [view, setView] = useState<"file" | "runs">("file");

  return (
    <div className="flex h-full">
      <nav className="flex w-14 shrink-0 flex-col items-center gap-1 border-r border-line bg-surface py-3">
        <span
          className="mb-3 grid size-8 place-items-center rounded-lg bg-accent text-bolt"
          title={info.data ? `Stormix v${info.data.version}` : "Stormix"}
        >
          <Zap size={17} fill="currentColor" />
        </span>
        <NavItem icon={<FileCode size={18} />} label="Test file" active={view === "file"} onClick={() => setView("file")} />
        <NavItem icon={<History size={18} />} label="Runs" active={view === "runs"} onClick={() => setView("runs")} />
        <div className="mt-auto text-[10px] text-subtle">{info.data ? `v${info.data.version}` : ""}</div>
      </nav>
      <main className="flex min-w-0 flex-1 flex-col">
        <UpdateBanner />
        {/* Kept mounted so the editor keeps its text while the history is open. */}
        <div className={view === "file" ? "min-h-0 flex-1" : "hidden"}>
          <TestFileView />
        </div>
        {view === "runs" && (
          <div className="min-h-0 flex-1">
            <RunsView />
          </div>
        )}
      </main>
    </div>
  );
}

function NavItem({
  icon,
  label,
  active,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  active?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      aria-current={active ? "page" : undefined}
      className={`group relative grid size-10 place-items-center rounded-lg ${
        active ? "bg-accent-soft text-accent" : "text-muted hover:bg-surface-2 hover:text-ink"
      }`}
    >
      {icon}
      <span className="nav-tip" role="tooltip">
        {label}
      </span>
    </button>
  );
}

/** Variables to run with, and the names of secrets to fill in by hand. */
type Env = { variables: Record<string, string>; secrets: string[] };

/** Edit a test file and see it parsed and checked as you type. */
function TestFileView() {
  const example = useQuery({ queryKey: ["example"], queryFn: commands.exampleTestFile });
  const [text, setText] = useState<string | null>(null);
  const [imported, setImported] = useState<ImportedCollection | null>(null);
  // Every environment met so far: a collection's, ones found next to an
  // opened file and ones imported from a file. They stay when another
  // file is opened, until removed.
  const [envs, setEnvs] = useState<Record<string, Env>>({});
  const [envName, setEnvName] = useState("");
  // Secret values typed in, by variable name. Never written to disk.
  const [secretValues, setSecretValues] = useState<Record<string, string>>({});
  const [envError, setEnvError] = useState<string | null>(null);
  const panel = usePanel();
  const [path, setPath] = useState<string | null>(null);
  const [savedText, setSavedText] = useState<string | null>(null);
  const [tab, setTab] = useState<"steps" | "flow" | "yaml">("steps");
  const [lastRun, setLastRun] = useState<FlowResult | null>(null);
  // What the steps editor shows while its YAML is being written.
  const [draft, setDraft] = useState<TestFile | null>(null);
  const writes = useRef(0);
  const client = useQueryClient();
  const source = text ?? example.data ?? "";
  const dirty = path != null && source !== savedText;
  const debounced = useDebounced(source, 250);

  const validation = useQuery({
    queryKey: ["validate", debounced],
    queryFn: () => commands.validateTestFile(debounced),
    enabled: debounced.length > 0,
    placeholderData: (previous) => previous,
  });
  const env = envs[envName] as Env | undefined;
  const envVars = useMemo(() => {
    const vars = { ...env?.variables };
    for (const name of env?.secrets ?? []) {
      if (secretValues[name]) vars[name] = secretValues[name];
    }
    return vars;
  }, [env, secretValues]);
  // The validator does not know the chosen environment; drop its warnings
  // about variables that environment provides.
  const result = useMemo(
    () =>
      validation.data && {
        ...validation.data,
        issues: validation.data.issues.filter((i) => {
          const name = /^`\{\{([^}]+)\}\}` is not defined/.exec(i.message)?.[1];
          return !(name && name in envVars);
        }),
      },
    [validation.data, envVars],
  );
  const canRun = !!result?.test && !result.issues.some((i) => i.severity === "error");

  const run = useMutation({
    mutationFn: async (yaml: string) => {
      const res = await commands.runTestFile(yaml, envVars, path);
      if (res.status === "error") throw new Error(res.error);
      return res.data;
    },
    // Kept past edits, so hovering a variable a step extracts shows its last value.
    onSuccess: (data) => setLastRun(data),
    onSettled: () => client.invalidateQueries({ queryKey: RUNS_KEY }),
  });
  const [points, setPoints] = useState<LoadProgress[]>([]);
  const load = useMutation({
    mutationFn: async (yaml: string) => {
      setPoints([]);
      const unlisten = await events.loadProgress.listen((e) => setPoints((p) => [...p, e.payload]));
      try {
        const res = await commands.runLoadTest(yaml, envVars, path);
        if (res.status === "error") throw new Error(res.error);
        return res.data;
      } finally {
        unlisten();
      }
    },
    onSettled: () => client.invalidateQueries({ queryKey: RUNS_KEY }),
  });
  const busy = run.isPending || load.isPending;
  /** Adds environments, replacing ones with the same name, and picks `select`. */
  const addEnvs = (added: Record<string, Env>, select: string | null) => {
    setEnvs((current) => ({ ...current, ...added }));
    if (select !== null) setEnvName(select);
    setEnvError(null);
    run.reset();
  };
  // What the hidden panel would show: errors or failures first.
  const panelAlert =
    result?.parseError ||
    result?.issues.some((i) => i.severity === "error") ||
    run.data?.steps.some((st) => !st.skipped && !stepPassed(st)) ||
    load.data?.thresholds.some((t) => !t.passed)
      ? "bad"
      : result?.issues.length
        ? "warn"
        : null;
  const shown = draft ?? result?.test ?? null;
  const variables = useMemo(
    () => variableTable(shown, envVars, envName, lastRun),
    [shown, envVars, envName, lastRun],
  );
  const edit = (next: string | null) => {
    writes.current += 1;
    setDraft(null);
    setText(next);
    run.reset();
    load.reset();
  };
  /** The steps editor changed the file: show it at once, then write its YAML. */
  const editSteps = async (next: TestFile) => {
    setDraft(next);
    run.reset();
    load.reset();
    const id = ++writes.current;
    const yaml = await commands.testFileToYaml(next as TestFile_Deserialize);
    if (id === writes.current) setText(yaml);
  };

  const file = useMutation({
    mutationFn: async (action: "open" | "save" | "saveAs") => {
      if (action === "open") {
        const picked = await open({
          title: "Open a test file",
          filters: [{ name: "Stormix test file", extensions: ["yaml", "yml"] }],
        });
        if (typeof picked !== "string") return;
        const res = await commands.readTextFile(picked);
        if (res.status === "error") throw new Error(res.error);
        setImported(null);
        edit(res.data);
        setPath(picked);
        setSavedText(res.data);
        const beside = await commands.environmentsBeside(picked);
        if (beside.status === "error") {
          setEnvError(`The environments next to this file could not be read: ${beside.error}`);
        } else if (beside.data.length > 0) {
          const names = beside.data.map((e) => e.name);
          addEnvs(
            Object.fromEntries(beside.data.map((e) => [e.name, { variables: e.variables, secrets: e.secrets }])),
            names.includes(envName) ? envName : names[0],
          );
        }
        return;
      }
      let target = action === "save" ? path : null;
      if (!target) {
        const name = (result?.test?.flow.name ?? "test").toLowerCase().replace(/[^a-z0-9]+/g, "-");
        target = await save({
          title: "Save the test file",
          defaultPath: `${name || "test"}.stormix.yaml`,
          filters: [{ name: "Stormix test file", extensions: ["yaml", "yml"] }],
        });
        if (!target) return;
      }
      const res = await commands.writeTextFile(target, source);
      if (res.status === "error") throw new Error(res.error);
      setPath(target);
      setSavedText(source);
    },
  });

  const importFrom = useMutation({
    mutationFn: async (kind: "postman" | "bruno") => {
      const path = await open(
        kind === "postman"
          ? { title: "Import a Postman collection", filters: [{ name: "Postman collection", extensions: ["json"] }] }
          : { title: "Import a Bruno collection folder", directory: true },
      );
      if (typeof path !== "string") return null;
      const res = await commands.importCollection(path);
      if (res.status === "error") throw new Error(res.error);
      return res.data;
    },
    onSuccess: (collection) => {
      if (!collection) return;
      setImported(collection);
      const names = Object.keys(collection.environments);
      addEnvs(
        Object.fromEntries(
          names.map((name) => [
            name,
            { variables: collection.environments[name], secrets: collection.secrets[name] ?? [] },
          ]),
        ),
        names.length > 0 ? names[0] : null,
      );
      setPath(null);
      edit(collection.yaml);
    },
    onError: () => panel.setOpen(true),
  });

  const editorPane = (
    <>
      <div className="flex items-center gap-1 border-b border-line bg-surface px-3 py-1.5">
        {(["steps", "flow", "yaml"] as const).map((t) => (
          <button
            key={t}
            type="button"
            onClick={() => setTab(t)}
            className={`rounded px-2 py-0.5 text-xs font-medium ${
              tab === t ? "bg-accent-soft text-accent" : "text-muted hover:bg-surface-2"
            }`}
          >
            {t === "steps" ? "Steps" : t === "flow" ? "Flow" : "YAML"}
          </button>
        ))}
        <span className="mono ml-3 min-w-0 truncate text-xs text-muted" title={path ?? undefined}>
          {path ? path.split(/[\\/]/).pop() : imported ? "Imported, not saved" : "Unsaved example"}
          {dirty && <span className="text-warn"> ●</span>}
        </span>
        <span className="ml-auto" />
        <FileButton label="Open" icon={<FolderOpen size={13} />} onClick={() => file.mutate("open")} />
        <FileButton label="Save" icon={<Save size={13} />} onClick={() => file.mutate("save")} />
        <FileButton label="Save as" onClick={() => file.mutate("saveAs")} />
        <PanelToggle
          open={panel.open}
          onClick={() => panel.setOpen(!panel.open)}
          alert={panelAlert}
        />
      </div>
      {file.error && <div className="border-b border-line bg-bad-soft px-3 py-1 text-xs text-bad">{file.error.message}</div>}
      {tab === "yaml" ? (
        <YamlEditor
          value={source}
          onChange={edit}
          validation={result}
          variables={variables}
          onSave={() => file.mutate("save")}
        />
      ) : shown && tab === "flow" ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <VariablesContext.Provider value={variables}>
            <FlowView
              test={shown}
              onChange={editSteps}
              lastRun={run.data ?? null}
              onOpenStep={(i) => {
                setTab("steps");
                // Once the Steps tab has rendered, bring that step into view.
                requestAnimationFrame(() =>
                  document.getElementById(`step-${i}`)?.scrollIntoView({ block: "start", behavior: "smooth" }),
                );
              }}
            />
          </VariablesContext.Provider>
        </div>
      ) : shown ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <VariablesContext.Provider value={variables}>
            <StepsEditor test={shown} onChange={editSteps} />
          </VariablesContext.Provider>
          <p className="px-4 pb-4 text-xs text-subtle">
            Editing here rewrites the YAML, so comments in it are dropped.
          </p>
        </div>
      ) : (
        <div className="p-4 text-muted">
          {result?.parseError ? "Fix the YAML first; the steps show once it parses." : "Loading…"}
        </div>
      )}
    </>
  );
  const resultsPane = (
    <>
      {result?.parseError ? (
        <Banner tone="bad" icon={<CircleAlert size={16} />} title="The file does not parse">
          <pre className="mono whitespace-pre-wrap text-xs">{result.parseError}</pre>
        </Banner>
      ) : result?.test ? (
        <>
          {envError && (
            <div className="mb-4">
              <Banner tone="bad" icon={<CircleAlert size={16} />} title="Environment">
                {envError}
              </Banner>
            </div>
          )}
          {importFrom.error && (
            <div className="mb-4">
              <Banner tone="bad" icon={<CircleAlert size={16} />} title="The collection could not be imported">
                {importFrom.error.message}
              </Banner>
            </div>
          )}
          {imported && imported.warnings.length > 0 && (
            <div className="mb-4">
              <Banner
                tone="warn"
                icon={<TriangleAlert size={16} />}
                title={`${count(imported.warnings.length, "thing")} did not carry over`}
              >
                <ul className="space-y-1">
                  {imported.warnings.map((w, i) => (
                    <li key={i}>{w}</li>
                  ))}
                </ul>
              </Banner>
            </div>
          )}
          <Issues issues={result.issues} />
          {(run.error ?? load.error) && (
            <div className="mt-4">
              <Banner tone="bad" icon={<CircleAlert size={16} />} title="The run could not start">
                {(run.error ?? load.error)?.message}
              </Banner>
            </div>
          )}
          {run.data && <RunResult result={run.data} />}
          {load.isPending && <LoadLive points={points} />}
          {load.data && <LoadResult report={load.data} />}
          <Summary test={result.test} />
        </>
      ) : null}
    </>
  );

  const importEnv = useMutation({
    mutationFn: async () => {
      const picked = await open({
        title: "Import an environment",
        filters: [
          { name: "Environment", extensions: ["yaml", "yml", "json", "bru"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (typeof picked !== "string") return null;
      const res = await commands.readEnvironment(picked);
      if (res.status === "error") throw new Error(res.error);
      return res.data;
    },
    onSuccess: (loaded) => {
      if (!loaded) return;
      addEnvs({ [loaded.name]: { variables: loaded.variables, secrets: loaded.secrets } }, loaded.name);
    },
    onError: (e) => {
      setEnvError(`The environment could not be imported: ${e.message}`);
      panel.setOpen(true);
    },
  });
  const removeEnv = (name: string) => {
    setEnvs(({ [name]: _, ...rest }) => rest);
    setEnvName("");
    run.reset();
  };
  const envNames = Object.keys(envs);

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 whitespace-nowrap border-b border-line bg-surface px-5 py-3">
        <h1 className="text-[15px] font-semibold">Test file</h1>
        <span className="min-w-0 truncate text-xs text-muted">
          A flow, an optional load profile and thresholds.
        </span>
        <span className="ml-auto text-xs text-muted">Import</span>
        <button
          type="button"
          disabled={importFrom.isPending}
          onClick={() => importFrom.mutate("postman")}
          className="flex items-center gap-1.5 rounded-md border border-line px-2.5 py-1 text-xs text-muted hover:bg-surface-2"
        >
          <FileJson size={13} />
          Postman
        </button>
        <button
          type="button"
          disabled={importFrom.isPending}
          onClick={() => importFrom.mutate("bruno")}
          className="flex items-center gap-1.5 rounded-md border border-line px-2.5 py-1 text-xs text-muted hover:bg-surface-2"
        >
          <FolderOpen size={13} />
          Bruno
        </button>
        <div className="flex items-center rounded-md border border-line">
          {envNames.length > 0 && (
            <select
              aria-label="Environment"
              value={envName}
              onChange={(e) => {
                setEnvName(e.target.value);
                run.reset();
              }}
              className="max-w-40 rounded-l-md border-r border-line bg-surface px-2 py-1 text-xs"
            >
              <option value="">No environment</option>
              {envNames.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
          )}
          {env && env.secrets.length > 0 && (
            <EnvSecrets
              names={env.secrets}
              values={secretValues}
              onChange={(name, value) => {
                setSecretValues((v) => ({ ...v, [name]: value }));
                run.reset();
              }}
            />
          )}
          {env && (
            <button
              type="button"
              aria-label={`Remove the ${envName} environment`}
              title="Remove this environment from the list"
              onClick={() => removeEnv(envName)}
              className="border-r border-line px-1.5 py-1 text-muted hover:bg-surface-2 hover:text-ink"
            >
              <X size={13} />
            </button>
          )}
          <button
            type="button"
            disabled={importEnv.isPending}
            onClick={() => importEnv.mutate()}
            title="Import an environment file: YAML or JSON variables, a Postman environment export or a Bruno .bru environment"
            className="flex items-center gap-1.5 rounded-r-md px-2.5 py-1 text-xs text-muted hover:bg-surface-2"
          >
            <FileUp size={13} />
            {envNames.length > 0 ? "Env" : "Import env"}
          </button>
        </div>
        <button
          type="button"
          onClick={() => {
            setImported(null);
            setPath(null);
            edit(null);
          }}
          className="flex items-center gap-1.5 rounded-md border border-line px-2.5 py-1 text-xs text-muted hover:bg-surface-2"
        >
          <FilePlus size={13} />
          New from example
        </button>
        <button
          type="button"
          disabled={!canRun || busy}
          onClick={() => {
            load.reset();
            panel.setOpen(true);
            run.mutate(source);
          }}
          className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1 text-xs font-semibold text-white hover:opacity-90 disabled:opacity-50"
        >
          {run.isPending ? <LoaderCircle size={13} className="animate-spin" /> : <Play size={13} />}
          Run once
        </button>
        <button
          type="button"
          disabled={!canRun || !result?.test?.load || busy}
          title={result?.test?.load ? describeLoad(result.test.load) : "Add a load section to run a load test"}
          onClick={() => {
            run.reset();
            panel.setOpen(true);
            load.mutate(source);
          }}
          className="flex items-center gap-1.5 rounded-md border border-accent px-3 py-1 text-xs font-semibold text-accent hover:bg-accent-soft disabled:opacity-50"
        >
          {load.isPending ? <LoaderCircle size={13} className="animate-spin" /> : <Gauge size={13} />}
          Load test
        </button>
      </header>
      <SplitView open={panel.open} width={panel.width} onWidth={panel.setWidth} main={editorPane} panel={resultsPane} />
    </div>
  );
}

function PanelToggle({ open, onClick, alert }: { open: boolean; onClick: () => void; alert: "bad" | "warn" | null }) {
  const label = open ? "Hide the results panel" : "Show the results panel";
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      aria-pressed={open}
      title={label}
      className="relative ml-1 grid size-6 place-items-center rounded text-muted hover:bg-surface-2 hover:text-ink"
    >
      {open ? <PanelRightClose size={15} /> : <PanelRightOpen size={15} />}
      {/* While the panel is hidden, a dot says it has something to show. */}
      {!open && alert && (
        <span className={`absolute right-0.5 top-0.5 size-1.5 rounded-full ${alert === "bad" ? "bg-bad" : "bg-warn"}`} />
      )}
    </button>
  );
}

function FileButton({ label, icon, onClick }: { label: string; icon?: ReactNode; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex items-center gap-1 rounded px-2 py-0.5 text-xs text-muted hover:bg-surface-2 hover:text-ink"
    >
      {icon}
      {label}
    </button>
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
  // What a run sends: turned off and set-aside steps are left out.
  const running = flow.steps.filter((s) => s.enabled !== false);
  const skipped = flow.steps.length - running.length;
  return (
    <section className="mt-4 rounded-lg border border-line bg-surface">
      <div className="border-b border-line px-4 py-3">
        <div className="font-semibold">{flow.name}</div>
        <div className="text-xs text-muted">{describeLoad(test.load)}</div>
      </div>
      <ol className="divide-y divide-line">
        {running.map((step, i) => (
          <li key={i} className="flex items-baseline gap-3 px-4 py-2">
            <span className="w-4 text-right text-xs text-subtle">{i + 1}</span>
            <span className={`mono w-14 shrink-0 text-xs font-semibold ${methodText(step.request.method)}`}>
              {step.request.method}
            </span>
            <div className="min-w-0">
              <div>{step.name}</div>
              <div className="mono truncate text-xs text-muted">{step.request.url}</div>
            </div>
          </li>
        ))}
      </ol>
      {skipped > 0 && (
        <div className="border-t border-line px-4 py-2 text-xs text-subtle">
          {skipped} step{skipped === 1 ? " is" : "s are"} turned off and will not run.
        </div>
      )}
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
