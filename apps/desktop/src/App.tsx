import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  ChevronDown,
  CircleAlert,
  CircleCheck,
  FileCode,
  FileJson,
  FilePlus,
  FileUp,
  FolderOpen,
  Gauge,
  History,
  Layers,
  LoaderCircle,
  PanelRightClose,
  PanelRightOpen,
  Play,
  Save,
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
import { describeLoad } from "./lib/describe";
import { methodText } from "./lib/methods";
import { VariablesContext, variableTable } from "./lib/variables";
import { newWorkspace, WORKSPACES_KEY, workspaceTitle } from "./lib/workspace";
import { type Activity, ActivityStrip, BusyIcon, Loading } from "./views/Busy";
import { LoadLive, LoadResult } from "./views/LoadResult";
import { LoadSetup, type LoadSetupChoice } from "./views/LoadSetup";
import { RunLive, RunResult, stepPassed } from "./views/RunResult";
import { RUNS_KEY, RunsView } from "./views/Runs";
import { SplitView, usePanel } from "./views/SidePanel";
import { applyOverrides, EnvEditor, type EnvOverrides } from "./views/EnvEditor";
import { EnvSecrets } from "./views/EnvSecrets";
import { FlowView } from "./views/FlowView";
import { StepsEditor } from "./views/StepsEditor";
import { UpdateBanner } from "./views/UpdateBanner";
import { WorkspacesView } from "./views/Workspaces";

export function App() {
  const info = useQuery({ queryKey: ["app-info"], queryFn: commands.getAppInfo });
  const client = useQueryClient();
  const list = useQuery({ queryKey: WORKSPACES_KEY, queryFn: commands.listWorkspaces });
  const [view, setView] = useState<"file" | "runs" | "workspaces">("file");
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
    if (latest.current && next !== "workspaces" && latest.current.view !== next) {
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
        <div className="mt-auto text-[0.625rem] text-subtle">{info.data ? `v${info.data.version}` : ""}</div>
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
            />
          )}
        </div>
        {view === "runs" && (
          <div className="min-h-0 flex-1">
            <RunsView />
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
}: {
  /** What to show at first; the view reports its changes to `onSnapshot`. */
  workspace: Workspace;
  workspaceName: string;
  onWorkspaces: () => void;
  onSnapshot: (snap: Snapshot) => void;
  secretValues: Record<string, string>;
  setSecretValues: Dispatch<SetStateAction<Record<string, string>>>;
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
  const [tab, setTab] = useState<"steps" | "flow" | "yaml">(
    ws.tab === "flow" || ws.tab === "yaml" ? ws.tab : "steps",
  );
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
  const run = useMutation({
    mutationFn: async (yaml: string) => {
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
    onSuccess: (data) => setLastRun(data),
    onSettled: () => {
      client.invalidateQueries({ queryKey: RUNS_KEY });
      client.invalidateQueries({ queryKey: CACHE_KEY });
    },
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
    onSettled: () => {
      client.invalidateQueries({ queryKey: RUNS_KEY });
      client.invalidateQueries({ queryKey: CACHE_KEY });
    },
  });
  const busy = run.isPending || load.isPending;
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
  /** Saves the setup's load profile and thresholds into the file, then runs the chosen steps. */
  const startLoad = async ({ included, cache, load: profile, thresholds }: LoadSetupChoice) => {
    const test = result?.test;
    if (!test) return;
    setSetupOpen(false);
    // Turned off steps are not offered, so they are not remembered as left out.
    setExcluded(
      test.flow.steps.filter((s, i) => s.enabled !== false && !included.includes(i)).map((s) => s.name),
    );
    const cacheChanged = test.flow.steps.some((s, i) => !!s.cache !== cache[i]);
    const configured = {
      ...test,
      load: profile,
      thresholds,
      flow: cacheChanged
        ? { ...test.flow, steps: test.flow.steps.map((s, i) => ({ ...s, cache: cache[i] })) }
        : test.flow,
    };
    if (cacheChanged || JSON.stringify([test.load, test.thresholds ?? []]) !== JSON.stringify([profile, thresholds])) {
      await editSteps(configured);
    }
    const steps = configured.flow.steps.filter((_, i) => included.includes(i));
    const yaml = await commands.testFileToYaml({ ...configured, flow: { ...configured.flow, steps } } as TestFile_Deserialize);
    run.reset();
    panel.setOpen(true);
    load.mutate(yaml);
  };
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
    () => variableTable(shown, envVars, envName, lastRun, sessionOnly),
    [shown, envVars, envName, lastRun, sessionOnly],
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
      const res = await showing({ label: "Saving", detail: at }, () => commands.writeTextFile(at, source));
      if (res.status === "error") throw new Error(res.error);
      setPath(target);
      setSavedText(source);
      setJustSaved(true);
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
      const what: Activity = {
        label: kind === "postman" ? "Importing the Postman collection" : "Importing the Bruno collection",
        detail: path,
      };
      // A Bruno folder reports each request file it reads.
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
  const fileBusy = file.isPending || importFrom.isPending || importEnv.isPending;

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
        <span className="mono ml-3 min-w-0 flex-1 basis-24 truncate text-xs text-muted" title={path ?? undefined}>
          {path
            ? path.split(/[\\/]/).pop()
            : imported
              ? "Imported, not saved"
              : text == null
                ? "Unsaved example"
                : "Not saved to a file"}
          {dirty && <span className="text-warn"> ●</span>}
          {justSaved && !dirty && (
            <span className="ml-1.5 inline-flex items-center gap-1 font-sans text-ok">
              <CircleCheck size="0.75rem" />
              Saved
            </span>
          )}
        </span>
        <FileButton
          label="Open"
          icon={<FolderOpen size="0.8125rem" />}
          busy={file.isPending && file.variables === "open"}
          disabled={fileBusy}
          onClick={() => file.mutate("open")}
        />
        <FileButton
          label="Save"
          icon={<Save size="0.8125rem" />}
          busy={file.isPending && file.variables !== "open"}
          disabled={fileBusy}
          onClick={() => file.mutate("save")}
        />
        <FileButton label="Save as" disabled={fileBusy} onClick={() => file.mutate("saveAs")} />
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
          onSave={() => !fileBusy && file.mutate("save")}
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
          {result?.parseError ? (
            "Fix the YAML first; the steps show once it parses."
          ) : (
            <Loading>Reading the test file…</Loading>
          )}
        </div>
      )}
    </>
  );
  const resultsPane = (
    <>
      {result?.parseError ? (
        <Banner tone="bad" icon={<CircleAlert size="1rem" />} title="The file does not parse">
          <pre className="mono whitespace-pre-wrap text-xs">{result.parseError}</pre>
        </Banner>
      ) : result?.test ? (
        <>
          {restoreNote && (
            <div className="mb-4">
              <Banner tone="warn" icon={<TriangleAlert size="1rem" />} title="Workspace">
                {restoreNote}
              </Banner>
            </div>
          )}
          {envError && (
            <div className="mb-4">
              <Banner tone="bad" icon={<CircleAlert size="1rem" />} title="Environment">
                {envError}
              </Banner>
            </div>
          )}
          {importFrom.error && (
            <div className="mb-4">
              <Banner tone="bad" icon={<CircleAlert size="1rem" />} title="The collection could not be imported">
                {importFrom.error.message}
              </Banner>
            </div>
          )}
          {imported && imported.warnings.length > 0 && (
            <div className="mb-4">
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
            </div>
          )}
          <Issues issues={result.issues} />
          {(run.error ?? load.error) && (
            <div className="mt-4">
              <Banner tone="bad" icon={<CircleAlert size="1rem" />} title="The run could not start">
                {(run.error ?? load.error)?.message}
              </Banner>
            </div>
          )}
          {run.isPending && <RunLive name={result.test.flow.name} progress={runStep} />}
          {run.data && <RunResult result={run.data} />}
          {load.isPending && <LoadLive points={points} />}
          {load.data && <LoadResult report={load.data} />}
          <Summary test={result.test} />
        </>
      ) : null}
    </>
  );

  const removeEnv = (name: string) => {
    setEnvs(({ [name]: _, ...rest }) => rest);
    setOverrides(({ [name]: _, ...rest }) => rest);
    setEnvName("");
    run.reset();
  };
  /** Changes a variable of the chosen environment for this session only. */
  const overrideVar = (name: string, value: string | null) => {
    setOverrides((all) => ({ ...all, [envName]: { ...all[envName], [name]: value } }));
    run.reset();
  };
  /** Drops this session's change to one variable, or to all of them. */
  const resetVar = (name: string | null) => {
    setOverrides(({ [envName]: current, ...rest }) => {
      if (name === null || !current) return rest;
      const { [name]: _, ...kept } = current;
      return Object.keys(kept).length ? { ...rest, [envName]: kept } : rest;
    });
    run.reset();
  };
  /** Writes this session's changes to these variables (all when null) to the environment's file. */
  const saveVars = async (names: string[] | null) => {
    if (!env) return;
    const changes = overrides[envName] ?? {};
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
  // Where an environment without a file of its own would be saved.
  const besidePath = useQuery({
    queryKey: ["env-path", path, envName],
    queryFn: () => commands.environmentPathBeside(path!, envName),
    enabled: !!path && !!env && !env.path,
  });
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
        ? lastPoint
          ? { label: "Load test running", done: lastPoint.elapsedS, total: lastPoint.totalS, unit: "seconds" }
          : { label: "Starting the load test" }
        : null);

  return (
    <div className="flex h-full flex-col">
      {/* Wraps onto more lines when the window is narrow or the text is large. */}
      <header className="flex flex-wrap items-center gap-x-3 gap-y-2 whitespace-nowrap border-b border-line bg-surface px-5 py-3">
        <div className="flex min-w-0 flex-1 basis-48 items-baseline gap-3">
          <h1 className="shrink-0 text-[0.9375rem] font-semibold">Test file</h1>
          <button
            type="button"
            onClick={onWorkspaces}
            title="This workspace is saved as you work. Open the list to switch or rename."
            className="flex min-w-0 items-center gap-1 self-center rounded-md border border-line px-2 py-0.5 text-xs text-muted hover:bg-surface-2 hover:text-ink"
          >
            <Layers size="0.75rem" className="shrink-0" />
            <span className="truncate">{workspaceName}</span>
            <ChevronDown size="0.75rem" className="shrink-0" />
          </button>
        </div>
        <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
          <span className="text-xs text-muted">Import</span>
          <button
            type="button"
            disabled={fileBusy}
            aria-busy={importFrom.isPending && importFrom.variables === "postman"}
            onClick={() => importFrom.mutate("postman")}
            className="flex items-center gap-1.5 rounded-md border border-line px-2.5 py-1 text-xs text-muted hover:bg-surface-2 disabled:cursor-default disabled:opacity-60 aria-busy:opacity-100 disabled:hover:bg-transparent"
          >
            <BusyIcon busy={importFrom.isPending && importFrom.variables === "postman"} icon={<FileJson size="0.8125rem" />} />
            Postman
          </button>
          <button
            type="button"
            disabled={fileBusy}
            aria-busy={importFrom.isPending && importFrom.variables === "bruno"}
            onClick={() => importFrom.mutate("bruno")}
            className="flex items-center gap-1.5 rounded-md border border-line px-2.5 py-1 text-xs text-muted hover:bg-surface-2 disabled:cursor-default disabled:opacity-60 aria-busy:opacity-100 disabled:hover:bg-transparent"
          >
            <BusyIcon busy={importFrom.isPending && importFrom.variables === "bruno"} icon={<FolderOpen size="0.8125rem" />} />
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
            {env && (
              <button
                type="button"
                onClick={() => setEnvEditorOpen(true)}
                title={
                  changedCount
                    ? `${changedCount} variable${changedCount === 1 ? "" : "s"} changed for this session only`
                    : "See and change this environment's variables"
                }
                className={`flex items-center gap-1.5 border-r border-line px-2 py-1 text-xs hover:bg-surface-2 ${
                  changedCount ? "text-warn" : "text-muted"
                }`}
              >
                <SlidersHorizontal size="0.8125rem" />
                Variables
                {changedCount > 0 && (
                  <span className="rounded-full bg-warn-soft px-1.5 text-[0.6875rem] font-semibold">{changedCount}</span>
                )}
              </button>
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
                <X size="0.8125rem" />
              </button>
            )}
            <button
              type="button"
              disabled={fileBusy}
              aria-busy={importEnv.isPending}
              onClick={() => importEnv.mutate()}
              title="Import an environment file: YAML or JSON variables, a Postman environment export or a Bruno .bru environment"
              className="flex items-center gap-1.5 rounded-r-md px-2.5 py-1 text-xs text-muted hover:bg-surface-2 disabled:cursor-default disabled:opacity-60 aria-busy:opacity-100"
            >
              <BusyIcon busy={importEnv.isPending} icon={<FileUp size="0.8125rem" />} />
              {envNames.length > 0 ? "Env" : "Import env"}
            </button>
          </div>
          <button
            type="button"
            disabled={fileBusy}
            onClick={() => {
              setImported(null);
              setPath(null);
              edit(null);
            }}
            className="flex items-center gap-1.5 rounded-md border border-line px-2.5 py-1 text-xs text-muted hover:bg-surface-2 disabled:cursor-default disabled:opacity-60 aria-busy:opacity-100"
          >
            <FilePlus size="0.8125rem" />
            New from example
          </button>
          <button
            type="button"
            disabled={!canRun || busy || fileBusy}
            onClick={() => {
              load.reset();
              panel.setOpen(true);
              run.mutate(source);
            }}
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
            run.reset();
          }}
          onClose={() => setEnvEditorOpen(false)}
        />
      )}
      {setupOpen && result?.test && (
        <LoadSetup
          test={result.test}
          excluded={excluded}
          onRun={startLoad}
          onCancel={() => setSetupOpen(false)}
        />
      )}
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
      {open ? <PanelRightClose size="0.9375rem" /> : <PanelRightOpen size="0.9375rem" />}
      {/* While the panel is hidden, a dot says it has something to show. */}
      {!open && alert && (
        <span className={`absolute right-0.5 top-0.5 size-1.5 rounded-full ${alert === "bad" ? "bg-bad" : "bg-warn"}`} />
      )}
    </button>
  );
}

function FileButton({
  label,
  icon,
  busy,
  disabled,
  onClick,
}: {
  label: string;
  icon?: ReactNode;
  busy?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-busy={busy}
      className="flex items-center gap-1 rounded px-2 py-0.5 text-xs text-muted hover:bg-surface-2 hover:text-ink disabled:cursor-default disabled:opacity-60 aria-busy:opacity-100 disabled:hover:bg-transparent disabled:hover:text-muted"
    >
      <BusyIcon busy={busy} icon={icon} />
      {label}
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
