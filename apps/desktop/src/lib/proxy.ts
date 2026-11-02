import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { commands, type ProxyChoice } from "../bindings";

const KEY = "stormix.proxy";
const QUERY_KEY = ["proxy"];

function saved(): ProxyChoice | null {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? (JSON.parse(raw) as ProxyChoice) : null;
  } catch {
    return null;
  }
}

/** Applies the proxy chosen in an earlier session, once at launch. */
export function useRestoreProxy() {
  const client = useQueryClient();
  useEffect(() => {
    const choice = saved();
    if (!choice || choice === "system") return;
    void commands.setProxy(choice).then(() => client.invalidateQueries({ queryKey: QUERY_KEY }));
  }, []);
}

/** The proxy requests go through, and a way to change it for this and later sessions. */
export function useProxy() {
  const client = useQueryClient();
  const current = useQuery({ queryKey: QUERY_KEY, queryFn: commands.getProxy });
  const change = useMutation({
    mutationFn: async (choice: ProxyChoice) => {
      const res = await commands.setProxy(choice);
      if (res.status === "error") throw new Error(res.error);
      try {
        localStorage.setItem(KEY, JSON.stringify(choice));
      } catch {
        // Applies to this session only.
      }
    },
    onSettled: () => client.invalidateQueries({ queryKey: QUERY_KEY }),
  });
  return { current: current.data ?? null, change };
}

export function describeProxy(choice: ProxyChoice): string {
  if (choice === "system") return "System";
  if (choice === "none") return "No proxy";
  return choice.url;
}
