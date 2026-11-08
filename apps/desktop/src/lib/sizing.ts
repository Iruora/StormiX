import type { Capacity, LimitSearch, LimitStep } from "../bindings";

/**
 * Pod sizing from a `find_limit` test, worked out the same way as the engine
 * (crates/engine/src/capacity.rs), so the result's calculator can try other
 * peaks and margins without running the test again.
 */

/** Every field filled in, as the engine reads the `capacity` section. */
export type CapacityInput = {
  peak_rps: number;
  growth: number;
  target_cpu: number;
  tested_pods: number;
  pod_cores: number | null;
  spare_pods: number;
  current_pods: number | null;
};

export const DEFAULT_CAPACITY: CapacityInput = {
  peak_rps: 100,
  growth: 30,
  target_cpu: 65,
  tested_pods: 1,
  pod_cores: null,
  spare_pods: 1,
  current_pods: null,
};

/** The file's `capacity` section with the engine's defaults filled in. */
export function capacityInput(c: Capacity | null | undefined): CapacityInput {
  if (!c) return DEFAULT_CAPACITY;
  return {
    peak_rps: c.peak_rps ?? DEFAULT_CAPACITY.peak_rps,
    growth: c.growth ?? DEFAULT_CAPACITY.growth,
    target_cpu: c.target_cpu ?? DEFAULT_CAPACITY.target_cpu,
    tested_pods: c.tested_pods ?? 1,
    pod_cores: c.pod_cores ?? null,
    spare_pods: c.spare_pods ?? 1,
    current_pods: c.current_pods ?? null,
  };
}

export type SizingResult = {
  testedRpsPerPod: number;
  testedCpu: number | null;
  /** Index of the step the CPU was scaled from. */
  cpuStep: number | null;
  podCores: number;
  usableRpsPerPod: number;
  designRps: number;
  podsForTraffic: number;
  pods: number;
  latencyMs: number;
  inFlightPerPod: number;
  inFlightTotal: number;
  basis: "cpu" | "latency" | "assumed";
};

/** A pod's CPU share, in percent, from which it counts as saturated. */
const SATURATED = 90;

export function sizePods(limit: LimitSearch, c: CapacityInput): SizingResult | null {
  const sustained = limit.sustained;
  if (sustained == null) return null;
  const steps = limit.steps as LimitStep[];
  const step = steps[sustained];
  if (!step) return null;
  const testedPods = Math.max(1, c.tested_pods || 1);
  const testedRpsPerPod = (step.rps ?? 0) / testedPods;
  if (testedRpsPerPod <= 0) return null;
  const podCores = c.pod_cores && c.pod_cores > 0 ? c.pod_cores : limit.cpuCores && limit.cpuCores > 0 ? limit.cpuCores : 1;
  const target = Math.min(100, Math.max(1, c.target_cpu || 0));
  const share = (s: LimitStep) => (s.apiCpu == null ? null : s.apiCpu / podCores);
  const testedCpu = share(step);
  let cpuStep: number | null = null;
  if (testedCpu != null) {
    cpuStep = sustained;
    if (testedCpu >= SATURATED) {
      for (let i = sustained - 1; i >= 0; i--) {
        const cpu = share(steps[i]);
        if (steps[i].passed && cpu != null && cpu > 0 && cpu < SATURATED) {
          cpuStep = i;
          break;
        }
      }
    }
  }
  let usableRpsPerPod: number;
  let basis: SizingResult["basis"];
  if (cpuStep != null) {
    const reference = steps[cpuStep];
    const cpu = share(reference) ?? 100;
    const rps = (reference.rps ?? 0) / testedPods;
    if (cpu > target) {
      usableRpsPerPod = Math.min((rps * target) / cpu, testedRpsPerPod);
      basis = "cpu";
    } else {
      usableRpsPerPod = testedRpsPerPod;
      basis = "latency";
    }
  } else {
    usableRpsPerPod = (testedRpsPerPod * target) / 100;
    basis = "assumed";
  }
  const designRps = Math.max(0, c.peak_rps || 0) * (1 + Math.max(0, c.growth || 0) / 100);
  const podsForTraffic = Math.max(1, Math.ceil(designRps / usableRpsPerPod));
  const latencyMs = step.latency.avg ?? 0;
  return {
    testedRpsPerPod,
    testedCpu,
    cpuStep,
    podCores,
    usableRpsPerPod,
    designRps,
    podsForTraffic,
    pods: podsForTraffic + Math.max(0, c.spare_pods || 0),
    latencyMs,
    inFlightPerPod: (usableRpsPerPod * latencyMs) / 1000,
    inFlightTotal: (designRps * latencyMs) / 1000,
    basis,
  };
}
