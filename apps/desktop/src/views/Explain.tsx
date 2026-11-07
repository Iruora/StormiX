import { Eye, EyeOff, RotateCcw, Settings2, Sparkles, Square } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";

import { commands, type AiQuestion_Deserialize, type AiReply } from "../bindings";
import { choiceOf, INSTALL_URL, PROVIDER_LABEL, PROVIDER_SHORT, useAiContext, useAiProviders } from "../lib/ai";
import { Spinner } from "./Busy";
import { CopyButton } from "./StepDetails";

/** Opens Settings, where the AI provider is chosen. */
export function openAiSettings() {
  window.dispatchEvent(new CustomEvent("stormix-open-settings"));
}

type State =
  | { kind: "idle" }
  | { kind: "asking" }
  | { kind: "answered"; reply: AiReply }
  | { kind: "failed"; error: string };

/**
 * Asks the user's own AI command line to explain a result, after hiding
 * secrets. Shows what is sent on request, and lets a slow answer be stopped.
 */
export function ExplainPanel({
  question,
  what,
  autoStart = false,
}: {
  /** Built when asked, so it holds the latest result. */
  question: () => AiQuestion_Deserialize;
  /** Like "why this step failed". */
  what: string;
  autoStart?: boolean;
}) {
  const { found, active, loading, settings } = useAiProviders();
  const { secrets } = useAiContext();
  const [state, setState] = useState<State>({ kind: "idle" });
  const [prompt, setPrompt] = useState<string | null>(null);
  const [showPrompt, setShowPrompt] = useState(false);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
      void commands.aiCancel();
    };
  }, []);

  const ask = async () => {
    setState({ kind: "asking" });
    const res = await commands.aiExplain(question(), secrets, choiceOf(settings), true);
    if (!live.current) return;
    if (res.status === "error") setState({ kind: "failed", error: res.error });
    else {
      setPrompt(res.data.prompt);
      setState({ kind: "answered", reply: res.data });
    }
  };
  const togglePrompt = async () => {
    if (!showPrompt && prompt == null) {
      const res = await commands.aiExplain(question(), secrets, choiceOf(settings), false);
      if (res.status === "ok") setPrompt(res.data.prompt);
    }
    setShowPrompt((s) => !s);
  };
  const started = useRef(false);
  useEffect(() => {
    if (autoStart && active && !started.current) {
      started.current = true;
      void ask();
    }
  }, [autoStart, active]);

  if (!settings.enabled) {
    return (
      <Note>
        AI help is turned off.{" "}
        <LinkButton onClick={openAiSettings}>Turn it on in Settings</LinkButton>.
      </Note>
    );
  }
  if (loading) {
    return (
      <Note>
        <Spinner className="mr-1.5 inline text-accent" /> Looking for Claude Code and GitHub Copilot CLI…
      </Note>
    );
  }
  if (!active) {
    return (
      <Note>
        {found.length === 0 ? (
          <>
            Stormix explains results with the AI command line you already use, so it needs no API key. Install{" "}
            <a className="text-accent underline" href={INSTALL_URL.claude} target="_blank" rel="noreferrer">
              Claude Code
            </a>{" "}
            or{" "}
            <a className="text-accent underline" href={INSTALL_URL.copilot} target="_blank" rel="noreferrer">
              GitHub Copilot CLI
            </a>{" "}
            and sign in once in a terminal, or{" "}
            <LinkButton onClick={openAiSettings}>set its path in Settings</LinkButton>.
          </>
        ) : (
          <>
            {settings.provider ? PROVIDER_LABEL[settings.provider] : "The chosen provider"} was not found.{" "}
            <LinkButton onClick={openAiSettings}>Choose another in Settings</LinkButton>.
          </>
        )}
      </Note>
    );
  }

  const name = PROVIDER_LABEL[active.provider];
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        {state.kind === "asking" ? (
          <>
            <span className="flex items-center gap-1.5 text-[0.8125rem] text-muted">
              <Spinner className="text-accent" /> Asking {name} {what}…
            </span>
            <button
              type="button"
              onClick={() => void commands.aiCancel()}
              className="flex items-center gap-1 rounded border border-line bg-surface px-2 py-0.5 text-xs text-muted hover:text-ink"
            >
              <Square size="0.6875rem" /> Stop
            </button>
          </>
        ) : (
          <button
            type="button"
            onClick={() => void ask()}
            className="flex items-center gap-1.5 rounded-md bg-accent px-2.5 py-1 text-xs font-medium text-white hover:opacity-90"
          >
            {state.kind === "idle" ? <Sparkles size="0.8125rem" /> : <RotateCcw size="0.8125rem" />}
            {state.kind === "idle" ? `Explain with ${PROVIDER_SHORT[active.provider]}` : "Ask again"}
          </button>
        )}
        <button
          type="button"
          onClick={() => void togglePrompt()}
          className="flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-muted hover:bg-surface-2 hover:text-ink"
        >
          {showPrompt ? <EyeOff size="0.75rem" /> : <Eye size="0.75rem" />}
          {showPrompt ? "Hide what is sent" : "Show what is sent"}
        </button>
        <button
          type="button"
          onClick={openAiSettings}
          title={`${name}${active.version ? ` ${active.version}` : ""}, at ${active.path}`}
          className="ml-auto flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-subtle hover:bg-surface-2 hover:text-ink"
        >
          <Settings2 size="0.75rem" /> {name}
          {settings.model.trim() && ` · ${settings.model.trim()}`}
        </button>
      </div>

      {state.kind === "idle" && (
        <p className="text-xs text-subtle">
          Runs {name} on this computer with your own account. Secrets, auth headers and tokens are hidden before
          anything is sent, and it gets no tools: it can only read what Stormix gives it.
        </p>
      )}
      {state.kind === "failed" && (
        <div className="rounded-md border border-bad/30 bg-bad-soft px-3 py-2 text-[0.8125rem] text-bad">{state.error}</div>
      )}
      {state.kind === "answered" && state.reply.answer && (
        <div className="rounded-md border border-line bg-surface-2 px-3 py-2.5">
          <AnswerText text={state.reply.answer.text} />
          <div className="mt-2 flex flex-wrap items-center gap-2 border-t border-line pt-2 text-xs text-subtle">
            <Sparkles size="0.75rem" />
            <span className="min-w-0 flex-1">
              Suggested by {PROVIDER_LABEL[state.reply.answer.provider]} in{" "}
              {((state.reply.answer.durationMs ?? 0) / 1000).toFixed(1)} s. Check it before acting on it.
            </span>
            <CopyButton label="Copy" text={() => state.reply.answer?.text ?? ""} />
          </div>
        </div>
      )}
      {showPrompt && prompt != null && (
        <div>
          <div className="mb-1 flex items-center gap-2 text-xs text-subtle">
            <span className="flex-1">
              Exactly what {name} receives. <span className="mono">‹hidden›</span> marks what was hidden.
            </span>
            <CopyButton label="Copy" text={() => prompt} />
          </div>
          <pre className="mono max-h-72 overflow-auto rounded-md bg-surface-2 px-3 py-2 text-xs leading-5 whitespace-pre-wrap break-all">
            {prompt}
          </pre>
        </div>
      )}
    </div>
  );
}

/** Plain text, with `- ` lines as a list and `code` spans set in mono. */
function AnswerText({ text }: { text: string }) {
  const blocks: ReactNode[] = [];
  let list: string[] = [];
  const flush = () => {
    if (list.length) {
      blocks.push(
        <ul key={blocks.length} className="ml-4 list-disc space-y-1">
          {list.map((l, i) => (
            <li key={i}>{inline(l)}</li>
          ))}
        </ul>,
      );
      list = [];
    }
  };
  for (const raw of text.split("\n")) {
    const line = raw.trimEnd();
    const item = /^\s*[-*•]\s+(.*)$/.exec(line);
    if (item) list.push(item[1]);
    else {
      flush();
      if (line.trim()) blocks.push(<p key={blocks.length}>{inline(line.replace(/^#+\s*/, ""))}</p>);
    }
  }
  flush();
  return <div className="space-y-2 text-[0.8125rem] leading-relaxed text-ink">{blocks}</div>;
}

function inline(text: string): ReactNode[] {
  return text.split(/(`[^`]+`|\*\*[^*]+\*\*)/g).map((part, i) =>
    part.startsWith("`") && part.endsWith("`") && part.length > 1 ? (
      <code key={i} className="mono rounded bg-surface px-1 text-xs">
        {part.slice(1, -1)}
      </code>
    ) : part.startsWith("**") && part.endsWith("**") && part.length > 3 ? (
      <strong key={i}>{part.slice(2, -2)}</strong>
    ) : (
      part
    ),
  );
}

function Note({ children }: { children: ReactNode }) {
  return <p className="text-[0.8125rem] text-muted">{children}</p>;
}

function LinkButton({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" onClick={onClick} className="text-accent underline">
      {children}
    </button>
  );
}
