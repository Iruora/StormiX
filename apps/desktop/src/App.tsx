import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  Check,
  ChevronDown,
  CircleAlert,
  CircleCheck,
  FileCode,
  FileJson,
  FilePlus,
  FileUp,
  FolderOpen,
  FolderOutput,
  Gauge,
  History,
  KeyRound,
  Layers,
  LoaderCircle,
  Menu as MenuIcon,
  PanelRightClose,
  PanelRightOpen,
  Play,
  Save,
  Search,
  Settings,
  SlidersHorizontal,
  TriangleAlert,
  X,
  Zap,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type Dispatch, type ReactNode, type SetStateAction } from "react";

import {
  commands,
  events,
  type FlowResult,
  type ImportedCollection,
  type Issue,
  type LoadProgress,
  type RunProgress,
  type TestFile,
  type TestFile_Deserialize,
  type Workspace_Serialize as Workspace,
} from "./bindings";
import { YamlEditor } from "./editor/YamlEditor";
import { CACHE_KEY } from "./lib/cache";
import { CurlContext, type CurlMaker, extractedBefore } from "./lib/curl";
import { describeLoad } from "./lib/describe";
import { methodText } from "./lib/methods";
import { VariableEditContext, type VariableEditing } from "./lib/variableEdit";
import { rebaseLinkedFiles } from "./lib/data";
import { VariablesContext, variableTable } from "./lib/variables";
import { newWorkspace, WORKSPACES_KEY, workspaceTitle } from "./lib/workspace";
import { type Activity, ActivityStrip, Loading } from "./views/Busy";
import { LoadLive, LoadResult, StopButton } from "./views/LoadResult";
import { LoadSetup, type LoadSetupChoice } from "./views/LoadSetup";
import { RunLive, RunResult, stepPassed } from "./views/RunResult";
import type { Diagnose } from "./views/StepDetails";
import { useProxy, useRestoreProxy } from "./lib/proxy";
import { RUNS_KEY, RunsView } from "./views/Runs";
import { SplitView, usePanel } from "./views/SidePanel";
import { applyOverrides, EnvEditor, type EnvOverrides } from "./views/EnvEditor";
import { FlowView } from "./views/FlowView";
import { StepsEditor, type StepsSelection } from "./views/StepsEditor";
import { UpdateBanner } from "./views/UpdateBanner";
import { WorkspacesView } from "./views/Workspaces";
import { ProxyButton } from "./views/ProxySetting";
import { type Command, CommandPalette } from "./views/CommandPalette";
import { Menu, MenuItem, MenuLabel, MenuSeparator, shortcut } from "./views/Menu";
import { SettingsView } from "./views/Settings";

export function App() {
  const info = useQuery({ queryKey: ["app-info"], queryFn: commands.getAppInfo });
  useRestoreProxy();
  const client = useQueryClient();
  const list = useQuery({ queryKey: WORKSPACES_KEY, queryFn: commands.listWorkspaces });
  const [view, setView] = useState<"file" | "runs" | "workspaces" | "settings">("file");
  // The workspace open in the editor, as it was when opened. The editor
  // reports its changes through `onSnapshot`; they are saved shortly after.
  const [active, setActive] = useState<Workspace | null>(null);
  const [activeName, setActiveName] = useState("");
  // Secret values typed in, by variable name. Kept in memory for the whole
  // session, across workspaces, and never saved.
  const [secretValues, setSecretValues] = useState<Record<string, string>>({});
  const latest = useRef<Workspace | null>(null);
  const pending = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const flush = async () => {
    clearTimeout(timer.current);
    if (!pending.current || !latest.current) return;
    pending.current = false;
    await commands.saveWorkspace(latest.current);
    client.invalidateQueries({ queryKey: WORKSPACES_KEY });
  };
  const persist = (ws: Workspace) => {
    latest.current = ws;
    pending.current = true;
    clearTimeout(timer.current);
    timer.current = setTimeout(flush, 400);
  };
  const flushRef = useRef(flush);
  flushRef.current = flush;

  // On launch, reopen the workspace used last, else start a new one.
  useEffect(() => {
    if (active) return;
    // Without the saved list, still let the user work.
    if (list.isError) openWorkspace(newWorkspace());
    if (!list.data) return;
    const pick = list.data.items.find((w) => w.id === list.data.last) ?? list.data.items[0] ?? newWorkspace();
    openWorkspace(pick);
  }, [list.data, list.isError]);

  // Save what is pending before the window closes.
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let unlisten: (() => void) | undefined;
    getCurrentWindow()
      .onCloseRequested(() => flushRef.current())
      .then((f) => (unlisten = f));
    return () => unlisten?.();
  }, []);

  /** Opens a workspace in the editor; the one open before is saved first. */
  const openWorkspace = (ws: Workspace) => {
    void flush();
    latest.current = ws;
    setActive(ws);
    setActiveName(ws.name);
    setView(ws.view === "runs" ? "runs" : "file");
    // A new one is saved once the editor reports what it shows.
    if (ws.updatedMs) void commands.selectWorkspace(ws.id);
  };
  const showView = (next: typeof view) => {
    setView(next);
    if (latest.current && (next === "file" || next === "runs") && latest.current.view !== next) {
      persist({ ...latest.current, view: next });
    }
  };
  const onSnapshot = (snap: Snapshot) => {
    const base = latest.current;
    if (!base) return;
    const { title, ...rest } = snap;
    const name = base.renamed ? base.name : title;
    setActiveName(name);
    persist({ ...base, ...rest, name });
  };
  const rename = async (ws: Workspace, name: string) => {
    if (ws.id === latest.current?.id) {
      latest.current = { ...latest.current, name, renamed: true };
      setActiveName(name);
      pending.current = true;
      await flush();
    } else {
      // Saving moves a workspace to the top and makes it the one reopened;
      // put the open one back there.
      await commands.saveWorkspace({ ...ws, name, renamed: true });
      if (latest.current) await commands.selectWorkspace(latest.current.id);
      client.invalidateQueries({ queryKey: WORKSPACES_KEY });
    }
  };
  const remove = async (ws: Workspace) => {
    await commands.deleteWorkspace(ws.id);
    if (ws.id === latest.current?.id) {
      pending.current = false;
      const rest = (list.data?.items ?? []).filter((w) => w.id !== ws.id);
      openWorkspace(rest[0] ?? newWorkspace());
      setView("workspaces");
    }
    client.invalidateQueries({ queryKey: WORKSPACES_KEY });
  };
  // The open workspace shows first, even before its first save.
  const items = useMemo(() => {
    const saved = list.data?.items ?? [];
    if (!active || saved.some((w) => w.id === active.id)) return saved;
    return [{ ...active, name: activeName }, ...saved];
  }, [list.data, active, activeName]);

  const appCommands: Command[] = [
    { id: "go-runs", group: "Go to", label: "Runs", keywords: "history", run: () => showView("runs") },
    { id: "go-workspaces", group: "Go to", label: "Workspaces", run: () => showView("workspaces") },
    { id: "go-settings", group: "Go to", label: "Settings", keywords: "proxy preferences", run: () => showView("settings") },
    ...items
      .filter((w) => w.id !== active?.id)
      .map((w) => ({ id: `ws-${w.id}`, group: "Switch workspace", label: w.name, run: () => openWorkspace(w) })),
  ];

  return (
    <div className="flex h-full">
      <nav className="flex w-14 shrink-0 flex-col items-center gap-1 border-r border-line bg-surface py-3">
        <span
          className="mb-3 grid size-8 place-items-center rounded-lg bg-accent text-bolt"
          title={info.data ? `Stormix v${info.data.version}` : "Stormix"}
        >
          <Zap size="1.0625rem" fill="currentColor" />
        </span>
        <NavItem icon={<FileCode size="1.125rem" />} label="Test file" active={view === "file"} onClick={() => showView("file")} />
        <NavItem icon={<History size="1.125rem" />} label="Runs" active={view === "runs"} onClick={() => showView("runs")} />
        <NavItem
          icon={<Layers size="1.125rem" />}
          label="Workspaces"
          active={view === "workspaces"}
          onClick={() => showView("workspaces")}
        />
        <div className="mt-auto" />
        <NavItem
          icon={<Settings size="1.125rem" />}
          label="Settings"
          active={view === "settings"}
          onClick={() => showView("settings")}
        />
        <div className="mt-1 text-[0.625rem] text-subtle">{info.data ? `v${info.data.version}` : ""}</div>
      </nav>
      <main className="flex min-w-0 flex-1 flex-col">
        <UpdateBanner />
        {/* Kept mounted so the editor keeps its text while another page is open. */}
        <div className={view === "file" ? "min-h-0 flex-1" : "hidden"}>
          {active && (
            <TestFileView
              key={active.id}
              workspace={active}
              workspaceName={activeName}
              onWorkspaces={() => showView("workspaces")}
              onSnapshot={onSnapshot}
              secretValues={secretValues}
              setSecretValues={setSecretValues}
              visible={view === "file"}
              appCommands={appCommands}
              onShowFile={() => showView("file")}
            />
          )}
        </div>
        {view === "runs" && (
          <div className="min-h-0 flex-1">
            <RunsView />
          </div>
        )}
        {view === "settings" && (
          <div className="min-h-0 flex-1">
            <SettingsView version={info.data?.version ?? null} />
          </div>
        )}
        {view === "workspaces" && (
          <div className="min-h-0 flex-1">
            <WorkspacesView
              items={items}
              activeId={active?.id ?? null}
              loading={list.isPending}
              onOpen={openWorkspace}
              onNew={() => openWorkspace(newWorkspace())}
              onRename={rename}
              onDelete={remove}
            />
          </div>
        )}
      </main>
    </div>
  );
}

/** What the editor reports for its workspace to be saved. */
type Snapshot = Pick<
  Workspace,
  "path" | "text" | "dirty" | "imported" | "environments" | "environment" | "tab" | "excluded"
> & { title: string };

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

/**
 * Variables to run with, the names of secrets to fill in by hand, and the
 * file it came from (null for a collection's, until saved).
 */
type Env = { variables: Record<string, string>; secrets: string[]; path: string | null };

/** Edit a test file and see it parsed and checked as you type. */
function TestFileView({
  workspace: ws,
  workspaceName,
  onWorkspaces,
  onSnapshot,
  secretValues,
  setSecretValues,
  visible,
  appCommands,
  onShowFile,
}: {
  /** What to show at first; the view reports its changes to `onSnapshot`. */
  workspace: Workspace;
  workspaceName: string;
  onWorkspaces: () => void;
  onSnapshot: (snap: Snapshot) => void;
  secretValues: Record<string, string>;
  setSecretValues: Dispatch<SetStateAction<Record<string, string>>>;
  /** Whether this page is the one shown; its shortcuts only work then. */
  visible: boolean;
  /** Going to the app's other pages, for the Ctrl K search. */
  appCommands: Command[];
  /** Brings this page to the front. */
  onShowFile: () => void;
}) {
  const example = useQuery({ queryKey: ["example"], queryFn: commands.exampleTestFile });
  const [text, setText] = useState<string | null>(ws.text);
  // A restored workspace only knows that its text was imported, not the
  // import's warnings.
  const [imported, setImported] = useState<ImportedCollection | null>(
    ws.imported ? { yaml: "", environments: {}, secrets: {}, warnings: [] } : null,
  );
  // Every environment met so far: a collection's, ones found next to an
  // opened file and ones imported from a file. They stay when another
  // file is opened, until removed.
  const [envs, setEnvs] = useState<Record<string, Env>>(ws.environments);
  const [envName, setEnvName] = useState(ws.environment in ws.environments ? ws.environment : "");
  const [envError, setEnvError] = useState<string | null>(null);
  // Changes to each environment for this session only, by environment name.
  // Runs use them; they reach the file only when saved.
  const [overrides, setOverrides] = useState<Record<string, EnvOverrides>>({});
  const [envEditorOpen, setEnvEditorOpen] = useState(false);
  const panel = usePanel();
  const [path, setPath] = useState<string | null>(ws.path);
  const [savedText, setSavedText] = useState<string | null>(ws.dirty ? null : ws.text);
  const [restoreNote, setRestoreNote] = useState<string | null>(null);
  // Which tab of the results panel is open.
  const [resultsTab, setResultsTab] = useState<"problems" | "run" | "summary">("problems");
  // The step the Steps tab shows, or the flow's settings.
  const [selectedStep, setSelectedStep] = useState<StepsSelection>(0);
  const [tab, setTab] = useState<"steps" | "flow" | "yaml">(
    ws.tab === "flow" || ws.tab === "yaml" ? ws.tab : "steps",
  );
  const [lastRun, setLastRun] = useState<FlowResult | null>(null);
  // The file the last run came from, so another file's values stay out of Copy as cURL.
  const [lastRunPath, setLastRunPath] = useState<string | null>(null);
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
    const vars = applyOverrides(env?.variables ?? {}, overrides[envName]);
    for (const name of env?.secrets ?? []) {
      if (secretValues[name]) vars[name] = secretValues[name];
    }
    return vars;
  }, [env, envName, overrides, secretValues]);
  const sessionOnly = useMemo(() => new Set(Object.keys(overrides[envName] ?? {})), [overrides, envName]);
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

  // The step a single run is on, as it reports them.
  const [runStep, setRunStep] = useState<RunProgress | null>(null);
  // The file each last run and load test came from, so Diagnose finds its
  // linked CSV files even after Save As moved the file.
  const ranFrom = useRef<{ run: string | null; load: string | null }>({ run: null, load: null });
  const run = useMutation({
    mutationFn: async (yaml: string) => {
      ranFrom.current.run = path;
      setRunStep(null);
      const unlisten = await events.runProgress.listen((e) => setRunStep(e.payload));
      try {
        const res = await commands.runTestFile(yaml, envVars, path);
        if (res.status === "error") throw new Error(res.error);
        return res.data;
      } finally {
        unlisten();
      }
    },
    // Kept past edits, so hovering a variable a step extracts shows its last value.
    onSuccess: (data) => {
      setLastRun(data);
      setLastRunPath(ranFrom.current.run);
    },
    onSettled: () => {
      client.invalidateQueries({ queryKey: RUNS_KEY });
      client.invalidateQueries({ queryKey: CACHE_KEY });
    },
  });
  // Diagnoses a step of the file a run was made from, with today's environment.
  const diagnoseIn =
    (yaml: string | undefined, from: string | null): Diagnose | undefined =>
    yaml === undefined
      ? undefined
      : async (step) => {
          const res = await commands.diagnoseStep(yaml, envVars, step, from);
          if (res.status === "error") throw new Error(res.error);
          return res.data;
        };
  const [points, setPoints] = useState<LoadProgress[]>([]);
  // Set once this view is left, so a load test still starting up never begins.
  const left = useRef(false);
  const load = useMutation({
    // `from`: the file the YAML's linked CSV files are relative to. A replay
    // passes the first run's, which Save As may since have moved.
    mutationFn: async ({ yaml, from }: { yaml: string; from: string | null }) => {
      ranFrom.current.load = from;
      setPoints([]);
      const unlisten = await events.loadProgress.listen((e) => setPoints((p) => [...p, e.payload]));
      try {
        if (left.current) throw new Error("the workspace was closed before the load test started");
        const res = await commands.runLoadTest(yaml, envVars, from);
        if (res.status === "error") throw new Error(res.error);
        return res.data;
      } finally {
        unlisten();
      }
    },
    onSettled: () => {
      client.invalidateQueries({ queryKey: RUNS_KEY });
      client.invalidateQueries({ queryKey: CACHE_KEY });
    },
  });
  // Leaving this workspace unmounts this view: stop its load test, which
  // would otherwise run on unseen, with no Stop button, and block the next one.
  const loadPending = useRef(false);
  useEffect(() => {
    loadPending.current = load.isPending;
  }, [load.isPending]);
  useEffect(() => {
    // Set again on mount: StrictMode unmounts and remounts once in development.
    left.current = false;
    return () => {
      left.current = true;
      if (loadPending.current) void commands.stopLoadTest().catch(() => {});
    };
  }, []);
  const busy = run.isPending || load.isPending;
  // Set from Stop until the stopped test returns what it measured.
  const [stopping, setStopping] = useState(false);
  useEffect(() => {
    if (!load.isPending) setStopping(false);
  }, [load.isPending]);
  /** Stops the running load test; its partial results then show, marked as stopped. */
  const stopLoad = () => {
    if (!load.isPending || stopping) return;
    setStopping(true);
    // False when the backend has not started the test yet: let Stop be pressed again.
    void commands.stopLoadTest().then((stopped) => {
      if (!stopped) setStopping(false);
    }, () => setStopping(false));
  };
  // A run that starts shows its progress.
  useEffect(() => {
    if (busy) setResultsTab("run");
  }, [busy]);
  // What a file, import or restore is doing now, for the activity strip.
  const [activity, setActivity] = useState<Activity | null>(null);
  /** Shows `what` in the activity strip while `work` runs. */
  const showing = async <T,>(what: Activity, work: () => Promise<T>): Promise<T> => {
    setActivity(what);
    try {
      return await work();
    } finally {
      setActivity(null);
    }
  };
  // A moment of "Saved" beside the file name after saving.
  const [justSaved, setJustSaved] = useState(false);
  useEffect(() => {
    if (!justSaved) return;
    const t = setTimeout(() => setJustSaved(false), 2000);
    return () => clearTimeout(t);
  }, [justSaved]);
  const [setupOpen, setSetupOpen] = useState(false);
  // Steps left out of the last load test, by name, offered again next time.
  const [excluded, setExcluded] = useState<string[]>(ws.excluded);
  /** Saves changed load, threshold, monitor, tracing and cache settings, then runs the chosen steps. */
  const startLoad = async ({ included, cache, load: profile, thresholds, trace, monitor, capacity, data }: LoadSetupChoice) => {
    const test = result?.test;
    if (!test) return;
    setSetupOpen(false);
    // Turned off steps are not offered, so they are not remembered as left out.
    setExcluded(
      test.flow.steps.filter((s, i) => s.enabled !== false && !included.includes(i)).map((s) => s.name),
    );
    const cacheChanged = test.flow.steps.some((s, i) => !!s.cache !== cache[i]);
    const traceChanged = (test.flow.trace ?? null) !== trace;
    const configured = {
      ...test,
      load: profile,
      thresholds,
      monitor,
      capacity,
      data,
      flow: {
        ...test.flow,
        trace,
        steps: cacheChanged ? test.flow.steps.map((s, i) => ({ ...s, cache: cache[i] })) : test.flow.steps,
      },
    };
    if (cacheChanged || traceChanged || JSON.stringify([test.load, test.thresholds ?? [], test.monitor ?? [], test.capacity ?? null, test.data ?? []]) !== JSON.stringify([profile, thresholds, monitor, capacity, data])) {
      await editSteps(configured);
    }
    const steps = configured.flow.steps.filter((_, i) => included.includes(i));
    const yaml = await commands.testFileToYaml({ ...configured, flow: { ...configured.flow, steps } } as TestFile_Deserialize);
    run.reset();
    panel.setOpen(true);
    load.mutate({ yaml, from: path });
  };
  /** Runs the last load test again: the same steps, load profile and thresholds. */
  const replayLoad = () => {
    const last = load.variables;
    if (!last || busy || fileBusy || !load.data) return;
    panel.setOpen(true);
    load.mutate(last);
  };
  /** Adds environments, replacing ones with the same name, and picks `select`. */
  const addEnvs = (added: Record<string, Env>, select: string | null) => {
    setEnvs((current) => ({ ...current, ...added }));
    if (select !== null) setEnvName(select);
    setEnvError(null);
    if (!run.isPending) run.reset();
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
    () => variableTable(shown, envVars, envName, lastRun, sessionOnly),
    [shown, envVars, envName, lastRun, sessionOnly],
  );
  // Copy as cURL fills in the same values a run would send now.
  const curlMaker = useMemo<CurlMaker | null>(
    () =>
      shown && {
        secrets: env?.secrets ?? [],
        make: async (step, shell) => {
          const extracted = lastRunPath === path ? extractedBefore(lastRun, step.name) : {};
          const res = await commands.stepAsCurl(shown, step, envVars, extracted, path, shell);
          if (res.status === "error") throw new Error(res.error);
          return res.data;
        },
      },
    [shown, env, envVars, lastRun, lastRunPath, path],
  );
  /**
   * Clears results that no longer match the file. A run still going is kept:
   * resetting it would hide its progress while the backend carries on.
   */
  const clearResults = () => {
    if (!run.isPending) run.reset();
    if (!load.isPending) load.reset();
  };
  const edit = (next: string | null) => {
    writes.current += 1;
    setDraft(null);
    setText(next);
    clearResults();
  };
  /** The steps editor changed the file: show it at once, then write its YAML. */
  const editSteps = async (next: TestFile) => {
    setDraft(next);
    clearResults();
    const id = ++writes.current;
    const yaml = await commands.testFileToYaml(next as TestFile_Deserialize);
    if (id === writes.current) setText(yaml);
  };

  // A restored workspace reads its file again, which may have changed
  // since, and the environments beside it. Unsaved changes are kept.
  useEffect(() => {
    if (!ws.path) return;
    const at = ws.path;
    let live = true;
    setActivity({ label: "Reopening", detail: at });
    (async () => {
      const res = await commands.readTextFile(at);
      if (!live) return;
      if (res.status === "error") {
        console.warn(res.error);
        setPath(null);
        setSavedText(null);
        setRestoreNote(
          `${at} could not be read; it may have been moved or deleted. The copy kept with this workspace is open instead: save it to keep it.`,
        );
        setActivity(null);
        return;
      }
      setSavedText(res.data);
      if (!ws.dirty) setText(res.data);
      setActivity({ label: "Reading the environments next to it", detail: at });
      const beside = await commands.environmentsBeside(at);
      if (!live) return;
      setActivity(null);
      if (beside.status === "error") return;
      setEnvs((current) => ({
        ...current,
        ...Object.fromEntries(
          beside.data.map((e) => [e.name, { variables: e.variables, secrets: e.secrets, path: e.path }]),
        ),
      }));
    })();
    return () => {
      live = false;
    };
  }, []);

  // Report what is open, for the workspace to be saved.
  const title = workspaceTitle(path, result?.test?.flow.name);
  useEffect(() => {
    onSnapshot({
      path,
      text,
      dirty,
      imported: imported != null && path == null,
      environments: envs,
      environment: envName,
      tab,
      excluded,
      title,
    });
  }, [path, text, dirty, imported, envs, envName, tab, excluded, title]);

  const file = useMutation({
    mutationFn: async (action: "open" | "save" | "saveAs") => {
      if (action === "open") {
        const picked = await open({
          title: "Open a test file",
          filters: [{ name: "Stormix test file", extensions: ["yaml", "yml"] }],
        });
        if (typeof picked !== "string") return;
        const res = await showing({ label: "Opening", detail: picked }, () => commands.readTextFile(picked));
        if (res.status === "error") throw new Error(res.error);
        setImported(null);
        edit(res.data);
        setPath(picked);
        setSavedText(res.data);
        const beside = await showing({ label: "Reading the environments next to it", detail: picked }, () =>
          commands.environmentsBeside(picked),
        );
        if (beside.status === "error") {
          setEnvError(`The environments next to this file could not be read: ${beside.error}`);
        } else if (beside.data.length > 0) {
          const names = beside.data.map((e) => e.name);
          addEnvs(
            Object.fromEntries(
              beside.data.map((e) => [e.name, { variables: e.variables, secrets: e.secrets, path: e.path }]),
            ),
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
      const at = target;
      // Linked CSV files are relative to the test file: keep them pointing at
      // the same files. Read from the text being saved, not the last
      // validation, which may lag behind the latest edit.
      const current = (await commands.validateTestFile(source)).test;
      const rebased = current && rebaseLinkedFiles(current.data ?? [], path, at);
      const text = rebased ? await commands.testFileToYaml({ ...current, data: rebased } as TestFile_Deserialize) : source;
      const res = await showing({ label: "Saving", detail: at }, () => commands.writeTextFile(at, text));
      if (res.status === "error") throw new Error(res.error);
      if (rebased) {
        // The steps editor's draft still has the old links: drop it, and any
        // of its YAML still being written, so the next edit starts from these.
        writes.current += 1;
        setDraft(null);
        setText(text);
      }
      setPath(target);
      setSavedText(text);
      setJustSaved(true);
    },
  });

  const importFrom = useMutation({
    mutationFn: async (kind: "postman" | "bruno" | "opencollection") => {
      const path = await open(
        kind === "postman"
          ? { title: "Import a Postman collection", filters: [{ name: "Postman collection", extensions: ["json"] }] }
          : kind === "opencollection"
            ? {
                title: "Import an OpenCollection: its opencollection.yml or a bundled file",
                filters: [{ name: "OpenCollection", extensions: ["yml", "yaml"] }],
              }
            : { title: "Import a Bruno collection folder (.bru or OpenCollection YAML)", directory: true },
      );
      if (typeof path !== "string") return null;
      const what: Activity = {
        label:
          kind === "postman"
            ? "Importing the Postman collection"
            : kind === "opencollection"
              ? "Importing the OpenCollection"
              : "Importing the Bruno collection",
        detail: path,
      };
      // A Bruno or OpenCollection folder reports each request file it reads.
      const unlisten = await events.importProgress.listen(({ payload: p }) =>
        setActivity({ ...what, detail: p.file, done: p.done, total: p.total, unit: "files" }),
      );
      try {
        const res = await showing(what, () => commands.importCollection(path));
        if (res.status === "error") throw new Error(res.error);
        return res.data;
      } finally {
        unlisten();
      }
    },
    onSuccess: (collection) => {
      if (!collection) return;
      setImported(collection);
      const names = Object.keys(collection.environments);
      addEnvs(
        Object.fromEntries(
          names.map((name) => [
            name,
            { variables: collection.environments[name], secrets: collection.secrets[name] ?? [], path: null },
          ]),
        ),
        names.length > 0 ? names[0] : null,
      );
      setPath(null);
      edit(collection.yaml);
    },
    onError: () => panel.setOpen(true),
  });

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
      const res = await showing({ label: "Importing the environment", detail: picked }, () =>
        commands.readEnvironment(picked),
      );
      if (res.status === "error") throw new Error(res.error);
      return res.data;
    },
    onSuccess: (loaded) => {
      if (!loaded) return;
      addEnvs(
        { [loaded.name]: { variables: loaded.variables, secrets: loaded.secrets, path: loaded.path } },
        loaded.name,
      );
    },
    onError: (e) => {
      setEnvError(`The environment could not be imported: ${e.message}`);
      panel.setOpen(true);
    },
  });
  // Opening, saving and importing touch the file shown; one at a time.
  // Writes the test file for another tool: an OpenCollection folder that
  // Bruno opens, one bundled OpenCollection file, or a Postman collection.
  const exportTo = useMutation({
    mutationFn: async (kind: "bruno" | "opencollection" | "postman") => {
      const base = (result?.test?.flow.name ?? "collection").toLowerCase().replace(/[^a-z0-9]+/g, "-") || "collection";
      const target =
        kind === "bruno"
          ? await open({ title: "Choose an empty folder for the Bruno collection", directory: true })
          : await save(
              kind === "postman"
                ? {
                    title: "Export a Postman collection",
                    defaultPath: `${base}.postman_collection.json`,
                    filters: [{ name: "Postman collection", extensions: ["json"] }],
                  }
                : {
                    title: "Export an OpenCollection file",
                    defaultPath: `${base}.yml`,
                    filters: [{ name: "OpenCollection", extensions: ["yml", "yaml"] }],
                  },
            );
      if (typeof target !== "string") return null;
      const environments = Object.entries(envs).map(([name, e]) => ({
        name,
        variables: e.variables,
        secrets: e.secrets,
      }));
      const label =
        kind === "bruno" ? "Exporting to Bruno" : kind === "postman" ? "Exporting to Postman" : "Exporting to OpenCollection";
      const res = await showing({ label, detail: target }, () =>
        kind === "bruno"
          ? commands.exportOpenCollection(source, environments, target)
          : kind === "postman"
            ? commands.exportPostman(source, environments, target)
            : commands.exportOpenCollectionFile(source, environments, target),
      );
      if (res.status === "error") throw new Error(res.error);
      const next =
        kind === "bruno"
          ? "Open that folder in Bruno."
          : kind === "postman"
            ? environments.length > 0
              ? "Import it in Postman, with the .postman_environment.json files written next to it."
              : "Import it in Postman."
            : "Open or import it in Bruno, or any tool that reads OpenCollection.";
      return { target, next, warnings: res.data };
    },
  });
  const fileBusy = file.isPending || importFrom.isPending || importEnv.isPending || exportTo.isPending;

  const newFromExample = () => {
    setImported(null);
    setPath(null);
    edit(null);
  };
  const runOnce = () => {
    if (!canRun || busy || fileBusy) return;
    load.reset();
    panel.setOpen(true);
    run.mutate(source);
  };
  const chooseEnv = (name: string) => {
    setEnvName(name);
    if (!run.isPending) run.reset();
  };
  /** Shows step `i` in the Steps tab. */
  const openStep = (i: number) => {
    setTab("steps");
    setSelectedStep(i);
  };
  const fileLabel = path
    ? path.split(/[\\/]/).pop()
    : imported
      ? "Imported, not saved"
      : text == null
        ? "Unsaved example"
        : "Not saved to a file";

  // Every action, for the Ctrl K search.
  const [paletteOpen, setPaletteOpen] = useState(false);
  const proxy = useProxy();
  const paletteCommands = (): Command[] => {
    const local: Command[] = [
      { id: "new", group: "File", label: "New from example", disabled: fileBusy, run: newFromExample },
      { id: "open", group: "File", label: "Open…", hint: shortcut("O"), disabled: fileBusy, run: () => file.mutate("open") },
      { id: "save", group: "File", label: "Save", hint: shortcut("S"), disabled: fileBusy, run: () => file.mutate("save") },
      { id: "save-as", group: "File", label: "Save as…", hint: shortcut("S", true), disabled: fileBusy, run: () => file.mutate("saveAs") },
      { id: "import-postman", group: "File", label: "Import a Postman collection…", disabled: fileBusy, run: () => importFrom.mutate("postman") },
      { id: "import-bruno", group: "File", label: "Import a Bruno folder…", disabled: fileBusy, run: () => importFrom.mutate("bruno") },
      {
        id: "import-oc",
        group: "File",
        label: "Import an OpenCollection file…",
        disabled: fileBusy,
        run: () => importFrom.mutate("opencollection"),
      },
      { id: "export-bruno", group: "File", label: "Export to Bruno (folder)…", disabled: fileBusy, run: () => exportTo.mutate("bruno") },
      {
        id: "export-oc",
        group: "File",
        label: "Export an OpenCollection file…",
        disabled: fileBusy,
        run: () => exportTo.mutate("opencollection"),
      },
      { id: "export-postman", group: "File", label: "Export a Postman collection…", disabled: fileBusy, run: () => exportTo.mutate("postman") },
      { id: "run", group: "Run", label: "Run once", disabled: !canRun || busy || fileBusy, run: runOnce },
      { id: "load", group: "Run", label: "Load test…", disabled: !canRun || busy || fileBusy, run: () => setSetupOpen(true) },
      ...(load.isPending
        ? [{ id: "load-stop", group: "Run", label: "Stop the load test", disabled: stopping, run: stopLoad }]
        : []),
      ...(load.data
        ? [{ id: "load-replay", group: "Run", label: "Replay the last load test", disabled: busy || fileBusy, run: replayLoad }]
        : []),
      ...envNames.map((name) => ({
        id: `env-${name}`,
        group: "Environment",
        label: `Use the ${name} environment`,
        hint: envName === name ? "in use" : undefined,
        run: () => chooseEnv(name),
      })),
      ...(envNames.length > 0
        ? [{ id: "env-none", group: "Environment", label: "Use no environment", hint: envName === "" ? "in use" : undefined, run: () => chooseEnv("") }]
        : []),
      ...(env
        ? [{ id: "env-edit", group: "Environment", label: "Variables and secrets…", run: () => setEnvEditorOpen(true) }]
        : []),
      { id: "env-import", group: "Environment", label: "Import an environment…", disabled: fileBusy, run: () => importEnv.mutate() },
      {
        id: "proxy-system",
        group: "Proxy",
        label: "Use the system proxy",
        hint: proxy.current === "system" ? "in use" : undefined,
        run: () => proxy.change.mutate("system"),
      },
      {
        id: "proxy-none",
        group: "Proxy",
        label: "Use no proxy",
        keywords: "direct off",
        hint: proxy.current === "none" ? "in use" : undefined,
        run: () => proxy.change.mutate("none"),
      },
      { id: "tab-steps", group: "View", label: "Show the steps", run: () => setTab("steps") },
      { id: "tab-flow", group: "View", label: "Show the flow", run: () => setTab("flow") },
      { id: "tab-yaml", group: "View", label: "Show the YAML", run: () => setTab("yaml") },
      {
        id: "panel",
        group: "View",
        label: panel.open ? "Hide the results panel" : "Show the results panel",
        run: () => panel.setOpen(!panel.open),
      },
      ...(shown?.flow.steps ?? []).map((step, i) => ({
        id: `step-${i}`,
        group: "Go to step",
        label: step.name,
        keywords: `${step.request.method} ${step.request.url}`,
        hint: step.request.method,
        run: () => openStep(i),
      })),
    ];
    // These are about the test file, so they show it first.
    return [...local.map((c) => ({ ...c, run: () => (onShowFile(), c.run()) })), ...appCommands];
  };
  // Shortcuts that work anywhere in the window, unless a focused editor took the key.
  const onKey = useRef<(e: KeyboardEvent) => void>(() => {});
  onKey.current = (e) => {
    if (e.defaultPrevented || !(e.ctrlKey || e.metaKey) || e.altKey) return;
    const key = e.key.toLowerCase();
    if (key === "k") {
      e.preventDefault();
      setPaletteOpen((o) => !o);
      return;
    }
    if (!visible || fileBusy) return;
    if (key === "o" && !e.shiftKey) {
      e.preventDefault();
      file.mutate("open");
    } else if (key === "s") {
      e.preventDefault();
      file.mutate(e.shiftKey ? "saveAs" : "save");
    }
  };
  useEffect(() => {
    const listener = (e: KeyboardEvent) => onKey.current(e);
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, []);

  // Where an environment without a file of its own would be saved.
  const besidePath = useQuery({
    queryKey: ["env-path", path, envName],
    queryFn: () => commands.environmentPathBeside(path!, envName),
    enabled: !!path && !!env && !env.path,
  });
  // What a variable's hover card uses to change a value in place. The
  // functions it calls are defined further down, so it calls them late.
  const variableEditing: VariableEditing = {
    envName: env ? envName : null,
    fileValues: env?.variables ?? {},
    secrets: env?.secrets ?? [],
    overrides: overrides[envName] ?? {},
    savesTo: env?.path ?? (path ? (besidePath.data ?? null) : null),
    setEnvVar: (name, value) => overrideVar(name, value),
    resetEnvVar: (name) => resetVar(name),
    saveEnvVar: (name, value) => saveVars([name], { [name]: value }),
    setSecret: (name, value) => {
      setSecretValues((v) => ({ ...v, [name]: value }));
      if (!run.isPending) run.reset();
    },
    setFlowVar: (name, value) => {
      if (shown) editSteps({ ...shown, flow: { ...shown.flow, variables: { ...shown.flow.variables, [name]: value } } });
    },
  };
  const editorPane = (
    <>
      <div className="flex flex-wrap items-center gap-1 whitespace-nowrap border-b border-line bg-surface px-3 py-1.5">
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
        <span className="flex-1" />
        <PanelToggle
          open={panel.open}
          onClick={() => panel.setOpen(!panel.open)}
          alert={panelAlert}
        />
      </div>
      {file.error && <div className="border-b border-line bg-bad-soft px-3 py-1 text-xs text-bad">{file.error.message}</div>}
      {exportTo.error && (
        <div className="border-b border-line bg-bad-soft px-3 py-1 text-xs text-bad">{exportTo.error.message}</div>
      )}
      {exportTo.data && (
        <div className="flex items-start gap-2 border-b border-line bg-surface-2 px-3 py-1 text-xs text-muted">
          <div className="min-w-0 flex-1">
            <div>
              Exported to <span className="break-all font-mono text-ink">{exportTo.data.target}</span>.{" "}
              {exportTo.data.next}
            </div>
            {exportTo.data.warnings.length > 0 && (
              <ul className="mt-0.5 list-disc pl-4 text-warn">
                {exportTo.data.warnings.map((w) => (
                  <li key={w}>{w}</li>
                ))}
              </ul>
            )}
          </div>
          <button
            type="button"
            aria-label="Dismiss"
            onClick={() => exportTo.reset()}
            className="rounded p-0.5 hover:bg-surface hover:text-ink"
          >
            <X size="0.75rem" />
          </button>
        </div>
      )}
      {tab === "yaml" ? (
        <YamlEditor
          value={source}
          onChange={edit}
          validation={result}
          variables={variables}
          onSave={() => !fileBusy && file.mutate("save")}
        />
      ) : shown && tab === "flow" ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <VariablesContext.Provider value={variables}>
            <VariableEditContext.Provider value={variableEditing}>
              <CurlContext.Provider value={curlMaker}>
                <FlowView
                  test={shown}
                  onChange={editSteps}
                  lastRun={run.data ?? null}
                  onOpenStep={openStep}
                />
              </CurlContext.Provider>
            </VariableEditContext.Provider>
          </VariablesContext.Provider>
        </div>
      ) : shown ? (
        <div className="min-h-0 flex-1">
          <VariablesContext.Provider value={variables}>
            <VariableEditContext.Provider value={variableEditing}>
              <CurlContext.Provider value={curlMaker}>
                <StepsEditor test={shown} onChange={editSteps} selected={selectedStep} onSelect={setSelectedStep} />
              </CurlContext.Provider>
            </VariableEditContext.Provider>
          </VariablesContext.Provider>
        </div>
      ) : (
        <div className="p-4 text-muted">
          {result?.parseError ? (
            "Fix the YAML first; the steps show once it parses."
          ) : (
            <Loading>Reading the test file…</Loading>
          )}
        </div>
      )}
    </>
  );
  // The results panel's tabs: what is wrong with the file, the last run, and what the file does.
  const problemErrors =
    (result?.parseError ? 1 : 0) +
    (envError ? 1 : 0) +
    (importFrom.error ? 1 : 0) +
    (result?.issues.filter((i) => i.severity === "error").length ?? 0);
  const problemCount =
    problemErrors +
    (restoreNote ? 1 : 0) +
    (imported && imported.warnings.length > 0 ? 1 : 0) +
    (result?.issues.filter((i) => i.severity !== "error").length ?? 0);
  const runFailed =
    !!(run.error ?? load.error) ||
    !!run.data?.steps.some((st) => !st.skipped && !stepPassed(st)) ||
    !!load.data?.thresholds.some((t) => !t.passed);
  const hasRun = busy || !!run.data || !!load.data || !!(run.error ?? load.error);
  const problemsPane = (
    <div className="space-y-4">
      {result?.parseError && (
        <Banner tone="bad" icon={<CircleAlert size="1rem" />} title="The file does not parse">
          <pre className="mono whitespace-pre-wrap text-xs">{result.parseError}</pre>
        </Banner>
      )}
      {restoreNote && (
        <Banner tone="warn" icon={<TriangleAlert size="1rem" />} title="Workspace">
          {restoreNote}
        </Banner>
      )}
      {envError && (
        <Banner tone="bad" icon={<CircleAlert size="1rem" />} title="Environment">
          {envError}
        </Banner>
      )}
      {importFrom.error && (
        <Banner tone="bad" icon={<CircleAlert size="1rem" />} title="The collection could not be imported">
          {importFrom.error.message}
        </Banner>
      )}
      {imported && imported.warnings.length > 0 && (
        <Banner
          tone="warn"
          icon={<TriangleAlert size="1rem" />}
          title={`${count(imported.warnings.length, "thing")} did not carry over`}
        >
          <ul className="space-y-1">
            {imported.warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </Banner>
      )}
      {result?.test && <Issues issues={result.issues} />}
    </div>
  );
  const runPane = (
    <>
      {(run.error ?? load.error) && (
        <div className="mb-4">
          <Banner tone="bad" icon={<CircleAlert size="1rem" />} title="The run could not start">
            {(run.error ?? load.error)?.message}
          </Banner>
        </div>
      )}
      {run.isPending && <RunLive name={result?.test?.flow.name ?? "the flow"} progress={runStep} />}
      {run.data && <RunResult result={run.data} diagnose={diagnoseIn(run.variables, ranFrom.current.run)} />}
      {load.isPending && <LoadLive points={points} onStop={stopLoad} stopping={stopping} />}
      {load.data && !load.isPending && (
        <LoadResult
          report={load.data}
          diagnose={diagnoseIn(load.variables?.yaml, ranFrom.current.load)}
          onReplay={busy || fileBusy ? undefined : replayLoad}
        />
      )}
      {!hasRun && (
        <p className="rounded-lg border border-dashed border-line px-4 py-6 text-center text-xs text-muted">
          Nothing run since the last change. <b className="font-semibold text-ink">Run once</b> sends every step one
          time; <b className="font-semibold text-ink">Load test</b> replays them under load. The results show here.
        </p>
      )}
    </>
  );
  const resultsPane = (
    <>
      <div
        role="tablist"
        aria-label="Results"
        className="sticky -top-4 z-10 -mx-4 -mt-4 mb-4 flex gap-1 border-b border-line bg-bg px-3 pt-2"
      >
        <ResultsTab
          active={resultsTab === "problems"}
          onClick={() => setResultsTab("problems")}
          label="Problems"
          badge={
            problemCount > 0 ? (
              <span className={problemErrors > 0 ? "text-bad" : "text-warn"}>{problemCount}</span>
            ) : (
              <CircleCheck size="0.75rem" className="text-ok" />
            )
          }
        />
        <ResultsTab
          active={resultsTab === "run"}
          onClick={() => setResultsTab("run")}
          label="Last run"
          badge={
            busy ? (
              <LoaderCircle size="0.75rem" className="animate-spin text-accent" />
            ) : hasRun ? (
              runFailed ? (
                <CircleAlert size="0.75rem" className="text-bad" />
              ) : (
                <CircleCheck size="0.75rem" className="text-ok" />
              )
            ) : null
          }
        />
        <ResultsTab active={resultsTab === "summary"} onClick={() => setResultsTab("summary")} label="Summary" />
      </div>
      {resultsTab === "problems" ? problemsPane : resultsTab === "run" ? runPane : result?.test ? <Summary test={result.test} /> : null}
    </>
  );

  const removeEnv = (name: string) => {
    setEnvs(({ [name]: _, ...rest }) => rest);
    setOverrides(({ [name]: _, ...rest }) => rest);
    setEnvName("");
    if (!run.isPending) run.reset();
  };
  /** Changes a variable of the chosen environment for this session only. */
  const overrideVar = (name: string, value: string | null) => {
    setOverrides((all) => ({ ...all, [envName]: { ...all[envName], [name]: value } }));
    if (!run.isPending) run.reset();
  };
  /** Drops this session's change to one variable, or to all of them. */
  const resetVar = (name: string | null) => {
    setOverrides(({ [envName]: current, ...rest }) => {
      if (name === null || !current) return rest;
      const { [name]: _, ...kept } = current;
      return Object.keys(kept).length ? { ...rest, [envName]: kept } : rest;
    });
    if (!run.isPending) run.reset();
  };
  /** Writes this session's changes to these variables (all when null) to the environment's file. */
  const saveVars = async (names: string[] | null, values?: EnvOverrides) => {
    if (!env) return;
    // Values passed in win over this session's: the hover saves what was just typed.
    const changes = { ...overrides[envName], ...values };
    const chosen = names ?? Object.keys(changes);
    let target = env.path;
    if (!target && path) target = await commands.environmentPathBeside(path, envName);
    if (!target) {
      target = await save({
        title: `Save the ${envName} environment`,
        defaultPath: `${envName}.yaml`,
        filters: [{ name: "Environment", extensions: ["yaml", "yml", "json", "bru"] }],
      });
      if (!target) return;
    }
    const picked = Object.fromEntries(chosen.filter((n) => n in changes).map((n) => [n, changes[n]]));
    const res = await commands.saveEnvironment(target, applyOverrides(env.variables, picked), env.secrets);
    if (res.status === "error") throw new Error(res.error);
    const saved = res.data;
    setEnvs((all) => ({ ...all, [envName]: { variables: saved.variables, secrets: saved.secrets, path: saved.path } }));
    setOverrides(({ [envName]: current, ...rest }) => {
      const kept = Object.fromEntries(Object.entries(current ?? {}).filter(([n]) => !chosen.includes(n)));
      return Object.keys(kept).length ? { ...rest, [envName]: kept } : rest;
    });
  };
  const changedCount = Object.keys(overrides[envName] ?? {}).length;
  const missingSecrets = env ? env.secrets.filter((n) => !secretValues[n]).length : 0;
  const envNames = Object.keys(envs);
  // Runs show in the strip too, for when the results panel is hidden.
  const lastPoint = points[points.length - 1];
  const strip: Activity | null =
    activity ??
    (run.isPending
      ? {
          label: `Running ${result?.test?.flow.name ?? "the flow"}`,
          detail: runStep?.name,
          done: runStep?.index,
          total: runStep?.total,
          unit: "steps done",
        }
      : load.isPending
        ? stopping
          ? { label: "Stopping the load test", detail: "letting the requests in flight finish" }
          : lastPoint
          ? { label: "Load test running", done: lastPoint.elapsedS, total: lastPoint.totalS, unit: "seconds" }
          : { label: "Starting the load test" }
        : null);

  return (
    <div className="flex h-full flex-col">
      {/* Wraps onto more lines when the window is narrow or the text is large. */}
      <header className="flex flex-wrap items-center gap-x-2 gap-y-2 whitespace-nowrap border-b border-line bg-surface px-4 py-2.5">
        <div className="flex min-w-0 flex-1 basis-72 items-center gap-2">
          <Menu label="File" icon={<MenuIcon size="0.8125rem" />} width="w-72">
            {(close) => (
              <>
                <MenuItem icon={<FilePlus size="0.8125rem" />} disabled={fileBusy} onClick={() => (close(), newFromExample())}>
                  New from example
                </MenuItem>
                <MenuItem
                  icon={<FolderOpen size="0.8125rem" />}
                  hint={shortcut("O")}
                  disabled={fileBusy}
                  onClick={() => (close(), file.mutate("open"))}
                >
                  Open…
                </MenuItem>
                <MenuItem
                  icon={<Save size="0.8125rem" />}
                  hint={shortcut("S")}
                  disabled={fileBusy}
                  onClick={() => (close(), file.mutate("save"))}
                >
                  Save
                </MenuItem>
                <MenuItem icon={null} hint={shortcut("S", true)} disabled={fileBusy} onClick={() => (close(), file.mutate("saveAs"))}>
                  Save as…
                </MenuItem>
                <MenuSeparator />
                <MenuLabel>Import a collection</MenuLabel>
                <MenuItem icon={<FileJson size="0.8125rem" />} disabled={fileBusy} onClick={() => (close(), importFrom.mutate("postman"))}>
                  Postman collection…
                </MenuItem>
                <MenuItem icon={<FolderOpen size="0.8125rem" />} disabled={fileBusy} onClick={() => (close(), importFrom.mutate("bruno"))}>
                  Bruno folder…
                </MenuItem>
                <MenuItem
                  icon={<FileCode size="0.8125rem" />}
                  disabled={fileBusy}
                  onClick={() => (close(), importFrom.mutate("opencollection"))}
                >
                  OpenCollection file…
                </MenuItem>
                <MenuSeparator />
                <MenuLabel>Export as</MenuLabel>
                <MenuItem icon={<FolderOutput size="0.8125rem" />} disabled={fileBusy} onClick={() => (close(), exportTo.mutate("bruno"))}>
                  Bruno folder…
                </MenuItem>
                <MenuItem icon={<FileCode size="0.8125rem" />} disabled={fileBusy} onClick={() => (close(), exportTo.mutate("opencollection"))}>
                  OpenCollection file…
                </MenuItem>
                <MenuItem icon={<FileJson size="0.8125rem" />} disabled={fileBusy} onClick={() => (close(), exportTo.mutate("postman"))}>
                  Postman collection…
                </MenuItem>
              </>
            )}
          </Menu>
          <button
            type="button"
            onClick={onWorkspaces}
            title="This workspace is saved as you work. Open the list to switch or rename."
            className="flex min-w-0 shrink items-center gap-1 rounded-md px-1.5 py-1 text-[0.8125rem] font-semibold hover:bg-surface-2"
          >
            <Layers size="0.8125rem" className="shrink-0 text-muted" />
            <span className="truncate">{workspaceName}</span>
            <ChevronDown size="0.75rem" className="shrink-0 text-muted" />
          </button>
          <span className="mono min-w-0 shrink truncate text-xs text-muted" title={path ?? undefined}>
            {fileLabel}
            {dirty && <span className="text-warn" title="Not saved yet"> ●</span>}
          </span>
          {justSaved && !dirty && (
            <span className="inline-flex shrink-0 items-center gap-1 text-xs text-ok">
              <CircleCheck size="0.75rem" />
              Saved
            </span>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={() => setPaletteOpen(true)}
            title="Search every action, environment and step"
            className="flex items-center gap-2 rounded-md border border-line px-2.5 py-1 text-xs text-subtle hover:bg-surface-2 hover:text-ink"
          >
            <Search size="0.8125rem" />
            Search
            <kbd className="font-sans text-[0.6875rem]">{shortcut("K")}</kbd>
          </button>
          <Menu
            label={
              <>
                <span className="max-w-40 truncate">{env ? envName : "No environment"}</span>
                {changedCount > 0 && <span className="font-semibold text-warn">· {changedCount} changed</span>}
              </>
            }
            icon={importEnv.isPending ? <LoaderCircle size="0.8125rem" className="animate-spin" /> : <SlidersHorizontal size="0.8125rem" />}
            tone={missingSecrets > 0 ? "warn" : "muted"}
            title={
              missingSecrets > 0
                ? `${count(missingSecrets, "secret")} without a value yet`
                : "Choose the environment, and see or change its variables"
            }
            align="right"
          >
            {(close) => (
              <>
                {envNames.length > 0 && (
                  <>
                    <MenuLabel>Environment</MenuLabel>
                    <MenuItem
                      icon={envName === "" ? <Check size="0.8125rem" /> : null}
                      checked={envName === ""}
                      onClick={() => (close(), chooseEnv(""))}
                    >
                      No environment
                    </MenuItem>
                    {envNames.map((name) => (
                      <MenuItem
                        key={name}
                        icon={envName === name ? <Check size="0.8125rem" /> : null}
                        checked={envName === name}
                        onClick={() => (close(), chooseEnv(name))}
                      >
                        {name}
                      </MenuItem>
                    ))}
                    <MenuSeparator />
                  </>
                )}
                {env && (
                  <>
                    <MenuItem
                      icon={<SlidersHorizontal size="0.8125rem" />}
                      hint={
                        changedCount > 0 ? (
                          <span className="rounded-full bg-warn-soft px-1.5 font-semibold text-warn">{changedCount} changed</span>
                        ) : undefined
                      }
                      onClick={() => (close(), setEnvEditorOpen(true))}
                    >
                      Variables…
                    </MenuItem>
                    {env.secrets.length > 0 && (
                      <MenuItem
                        icon={<KeyRound size="0.8125rem" />}
                        hint={
                          <span className={missingSecrets > 0 ? "text-warn" : undefined}>
                            {env.secrets.length - missingSecrets} of {env.secrets.length} set
                          </span>
                        }
                        onClick={() => (close(), setEnvEditorOpen(true))}
                      >
                        Secrets…
                      </MenuItem>
                    )}
                    <MenuSeparator />
                  </>
                )}
                <MenuItem icon={<FileUp size="0.8125rem" />} disabled={fileBusy} onClick={() => (close(), importEnv.mutate())}>
                  Import environment…
                </MenuItem>
                {env && (
                  <MenuItem icon={<X size="0.8125rem" />} tone="bad" onClick={() => (close(), removeEnv(envName))}>
                    Remove {envName} from the list
                  </MenuItem>
                )}
              </>
            )}
          </Menu>
          <ProxyButton />
          <span className="mx-0.5 h-5 w-px bg-line" aria-hidden />
          <button
            type="button"
            disabled={!canRun || busy || fileBusy}
            onClick={runOnce}
            aria-busy={run.isPending}
            className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1 text-xs font-semibold text-white hover:opacity-90 disabled:opacity-50 aria-busy:opacity-100"
          >
            {run.isPending ? <LoaderCircle size="0.8125rem" className="animate-spin" /> : <Play size="0.8125rem" />}
            Run once
          </button>
          <button
            type="button"
            disabled={!canRun || busy || fileBusy}
            title={result?.test?.load ? describeLoad(result.test.load) : "Choose the endpoints and load profile, then run"}
            onClick={() => setSetupOpen(true)}
            aria-busy={load.isPending}
            className="flex items-center gap-1.5 rounded-md border border-accent px-3 py-1 text-xs font-semibold text-accent hover:bg-accent-soft disabled:opacity-50 aria-busy:opacity-100"
          >
            {load.isPending ? <LoaderCircle size="0.8125rem" className="animate-spin" /> : <Gauge size="0.8125rem" />}
            Load test
          </button>
          {load.isPending && <StopButton onClick={stopLoad} stopping={stopping} />}
        </div>
      </header>
      <ActivityStrip activity={strip} />
      {envEditorOpen && env && (
        <EnvEditor
          name={envName}
          variables={env.variables}
          secrets={env.secrets}
          overrides={overrides[envName] ?? {}}
          secretValues={secretValues}
          savesTo={env.path ?? (path ? (besidePath.data ?? null) : null)}
          onOverride={overrideVar}
          onReset={resetVar}
          onSave={saveVars}
          onSecret={(name, value) => {
            setSecretValues((v) => ({ ...v, [name]: value }));
            if (!run.isPending) run.reset();
          }}
          onClose={() => setEnvEditorOpen(false)}
        />
      )}
      {paletteOpen && <CommandPalette commands={paletteCommands()} onClose={() => setPaletteOpen(false)} />}
      {setupOpen && result?.test && (
        <LoadSetup
          test={result.test}
          excluded={excluded}
          filePath={path}
          onRun={startLoad}
          onCancel={() => setSetupOpen(false)}
        />
      )}
      <SplitView open={panel.open} width={panel.width} onWidth={panel.setWidth} main={editorPane} panel={resultsPane} />
    </div>
  );
}

function ResultsTab({
  active,
  onClick,
  label,
  badge,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
  badge?: ReactNode;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={`-mb-px flex items-center gap-1.5 border-b-2 px-2.5 pb-1.5 pt-1 text-xs font-medium ${
        active ? "border-accent text-accent" : "border-transparent text-muted hover:text-ink"
      }`}
    >
      {label}
      {badge && <span className="flex items-center text-[0.6875rem] font-semibold">{badge}</span>}
    </button>
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
      {open ? <PanelRightClose size="0.9375rem" /> : <PanelRightOpen size="0.9375rem" />}
      {/* While the panel is hidden, a dot says it has something to show. */}
      {!open && alert && (
        <span className={`absolute right-0.5 top-0.5 size-1.5 rounded-full ${alert === "bad" ? "bg-bad" : "bg-warn"}`} />
      )}
    </button>
  );
}

function Issues({ issues }: { issues: Issue[] }) {
  if (issues.length === 0) {
    return (
      <Banner tone="ok" icon={<CircleCheck size="1rem" />} title="Valid">
        No problems found.
      </Banner>
    );
  }
  const errors = issues.filter((i) => i.severity === "error").length;
  return (
    <Banner
      tone={errors > 0 ? "bad" : "warn"}
      icon={errors > 0 ? <CircleAlert size="1rem" /> : <TriangleAlert size="1rem" />}
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
    <section className="rounded-lg border border-line bg-surface">
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
            <div className="min-w-0 flex-1">
              <div className="break-words">{step.name}</div>
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
      <div className="mt-1.5 text-[0.8125rem] text-ink">{children}</div>
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
