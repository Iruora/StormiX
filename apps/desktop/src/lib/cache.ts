import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { type CachedStep, commands } from "../bindings";

/** Refreshed after every run and load test, which can fill the cache. */
export const CACHE_KEY = ["cache"];

/**
 * What the app's response cache holds for one flow, by step name, and a way
 * to clear one step or all of them. The cache lives in the app, not the file:
 * it starts empty each launch.
 */
export function useResponseCache(flow: string) {
  const client = useQueryClient();
  const query = useQuery({ queryKey: CACHE_KEY, queryFn: commands.cachedSteps });
  const steps = new Map<string, CachedStep>();
  for (const s of query.data ?? []) if (s.flow === flow) steps.set(s.step, s);
  const clear = useMutation({
    mutationFn: (step: string | null) => commands.clearCache(flow, step),
    onSettled: () => client.invalidateQueries({ queryKey: CACHE_KEY }),
  });
  return {
    steps,
    /** Forgets one step's responses, or with null, every step's in this flow. */
    clear: (step: string | null) => clear.mutate(step),
  };
}

/** Like `2 min ago`. */
export function storedAgo(entry: CachedStep): string {
  const seconds = Math.max(0, Math.round((Date.now() - (entry.storedAt ?? Date.now())) / 1000));
  if (seconds < 60) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  return hours < 24 ? `${hours} h ago` : `${Math.round(hours / 24)} d ago`;
}

/** A tooltip for a step with responses kept. */
export function cachedTitle(entry: CachedStep): string {
  const n = entry.responses;
  return `${n === 1 ? "A response is" : `${n} responses are`} cached, kept ${storedAgo(entry)}. Runs reuse ${
    n === 1 ? "it" : "them"
  } instead of sending the request.`;
}
