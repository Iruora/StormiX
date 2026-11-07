import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createContext, useCallback, useContext, useEffect, useState } from "react";

import { commands, type AiChoice, type AiPath, type Found, type Provider } from "../bindings";

const KEY = "stormix.ai";
const CHANGED = "stormix-ai-settings";

/** Which AI command line to ask, kept between sessions. */
export type AiSettings = {
  /** `null` asks the first one found. */
  provider: Provider | null;
  /** Empty keeps the provider's own default. */
  model: string;
  /** Program paths set by hand, for a provider not on PATH. */
  paths: Partial<Record<Provider, string>>;
  /** Off hides every AI button. */
  enabled: boolean;
};

const DEFAULTS: AiSettings = { provider: null, model: "", paths: {}, enabled: true };

function read(): AiSettings {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? { ...DEFAULTS, ...(JSON.parse(raw) as Partial<AiSettings>) } : DEFAULTS;
  } catch {
    return DEFAULTS;
  }
}

export const PROVIDER_LABEL: Record<Provider, string> = {
  claude: "Claude Code",
  copilot: "GitHub Copilot CLI",
};

export const PROVIDER_SHORT: Record<Provider, string> = {
  claude: "Claude",
  copilot: "Copilot",
};

export const INSTALL_URL: Record<Provider, string> = {
  claude: "https://docs.claude.com/en/docs/claude-code/setup",
  copilot: "https://docs.github.com/en/copilot/how-tos/set-up/install-copilot-cli",
};

/** The AI settings, and a way to change them for this and later sessions. */
export function useAiSettings(): [AiSettings, (next: Partial<AiSettings>) => void] {
  const [settings, setSettings] = useState(read);
  useEffect(() => {
    const sync = () => setSettings(read());
    window.addEventListener(CHANGED, sync);
    return () => window.removeEventListener(CHANGED, sync);
  }, []);
  const change = useCallback((next: Partial<AiSettings>) => {
    const merged = { ...read(), ...next };
    try {
      localStorage.setItem(KEY, JSON.stringify(merged));
    } catch {
      // Applies to this session only.
    }
    setSettings(merged);
    window.dispatchEvent(new Event(CHANGED));
  }, []);
  return [settings, change];
}

function pathsOf(settings: AiSettings): AiPath[] {
  return (Object.entries(settings.paths) as [Provider, string][])
    .filter(([, path]) => path.trim())
    .map(([provider, path]) => ({ provider, path }));
}

export function choiceOf(settings: AiSettings): AiChoice {
  return { provider: settings.provider, paths: pathsOf(settings), model: settings.model.trim() || null };
}

/** The AI command lines found on this machine, looked for once per launch. */
export function useAiProviders() {
  const [settings] = useAiSettings();
  const paths = pathsOf(settings);
  const client = useQueryClient();
  const key = ["ai-providers", paths];
  const query = useQuery({
    queryKey: key,
    queryFn: () => commands.aiProviders(paths),
    staleTime: Infinity,
  });
  const found: Found[] = query.data ?? [];
  const active = settings.enabled
    ? (found.find((f) => f.provider === settings.provider) ?? (settings.provider ? undefined : found[0]))
    : undefined;
  return {
    found,
    active: active ?? null,
    loading: query.isPending,
    refresh: () => client.invalidateQueries({ queryKey: ["ai-providers"] }),
    settings,
  };
}

/**
 * What the AI may see besides the result itself: the test file the result
 * came from, and values to hide (secrets and variables named like them).
 */
export type AiContextValue = { file: string | null; secrets: string[] };

export const AiContext = createContext<AiContextValue>({ file: null, secrets: [] });

export function useAiContext() {
  return useContext(AiContext);
}

/** Variable names that likely hold a credential, to hide their values. */
export function isSecretName(name: string): boolean {
  return /pass(word|wd)?|secret|token|api[-_]?key|credential|private|cookie|session/i.test(name);
}
