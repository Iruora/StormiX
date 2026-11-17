/**
 * Time the API was unhealthy, so a short outage that broke `unhealthy <= 0s`
 * never reads as `0 s`: milliseconds below 1 s, tenths below 10 s.
 */
export function unhealthyText(seconds: number): string {
  if (seconds <= 0) return "0 s";
  if (seconds < 1) return `${Math.max(1, Math.round(seconds * 1000))} ms`;
  if (seconds < 10) return `${seconds.toFixed(1)} s`;
  return `${Math.round(seconds)} s`;
}
