/**
 * One colour per HTTP method, the way API tools usually show them: reads
 * green, creates amber, replaces blue, patches violet, deletes red.
 */
const TONES: Record<string, string> = {
  GET: "text-emerald-700 bg-emerald-50 border-emerald-200 dark:text-emerald-300 dark:bg-emerald-950/60 dark:border-emerald-800",
  POST: "text-amber-700 bg-amber-50 border-amber-200 dark:text-amber-300 dark:bg-amber-950/60 dark:border-amber-800",
  PUT: "text-sky-700 bg-sky-50 border-sky-200 dark:text-sky-300 dark:bg-sky-950/60 dark:border-sky-800",
  PATCH: "text-violet-700 bg-violet-50 border-violet-200 dark:text-violet-300 dark:bg-violet-950/60 dark:border-violet-800",
  DELETE: "text-red-700 bg-red-50 border-red-200 dark:text-red-300 dark:bg-red-950/60 dark:border-red-800",
  HEAD: "text-slate-600 bg-slate-100 border-slate-200 dark:text-slate-300 dark:bg-slate-800/60 dark:border-slate-700",
  OPTIONS: "text-pink-700 bg-pink-50 border-pink-200 dark:text-pink-300 dark:bg-pink-950/60 dark:border-pink-800",
};

const TEXT: Record<string, string> = {
  GET: "text-emerald-700 dark:text-emerald-300",
  POST: "text-amber-700 dark:text-amber-300",
  PUT: "text-sky-700 dark:text-sky-300",
  PATCH: "text-violet-700 dark:text-violet-300",
  DELETE: "text-red-700 dark:text-red-300",
  HEAD: "text-slate-600 dark:text-slate-300",
  OPTIONS: "text-pink-700 dark:text-pink-300",
};

/** Text, background and border classes for a method badge or selector. */
export function methodTone(method: string): string {
  return TONES[method.toUpperCase()] ?? "text-accent bg-accent-soft border-line";
}

/** Just the text colour, for a method written inline. */
export function methodText(method: string): string {
  return TEXT[method.toUpperCase()] ?? "text-accent";
}
