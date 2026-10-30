import type { LoadProfile } from "../bindings";

/** One line summing up a load profile, e.g. "Ramping 0 → 20/s over 3 stages, 1m45s". */
export function describeLoad(load: LoadProfile | null | undefined): string {
  if (!load) return "Runs once as a functional test";
  if ("constant_rate" in load && load.constant_rate) {
    const { rate, duration } = load.constant_rate;
    return `Constant ${rate} iterations/s for ${duration}`;
  }
  if ("ramping" in load && load.ramping) {
    const { start_rate, stages } = load.ramping;
    const peak = Math.max(start_rate ?? 0, ...stages.map((s) => s.target));
    const plural = stages.length === 1 ? "stage" : "stages";
    return `Ramping up to ${peak} iterations/s over ${stages.length} ${plural}`;
  }
  if ("virtual_users" in load && load.virtual_users) {
    const { count, duration, think_time } = load.virtual_users;
    const think = think_time ? `, ${think_time} think time` : "";
    return `${count} virtual users for ${duration}${think}`;
  }
  return "Unknown load profile";
}
