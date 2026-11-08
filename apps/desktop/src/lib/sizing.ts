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
  /** The pod's memory limit in MiB; sizes memory too when set. */
  pod_memory_mb: number | null;
  target_memory: number;
  spare_pods: number;
  current_pods: number | null;
};

export const DEFAULT_CAPACITY: CapacityInput = {
  peak_rps: 100,
  growth: 30,
  target_cpu: 65,
  tested_pods: 1,
  pod_cores: null,
  pod_memory_mb: null,
  target_memory: 80,
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
    pod_memory_mb: c.pod_memory_mb ?? null,
    target_memory: c.target_memory ?? 80,
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
  podsByCpu: number;
  podsByMemory: number | null;
  fixedMemoryBytes: number | null;
  memoryPerRequestBytes: number | null;
  memoryNote: string | null;
  basis: "cpu" | "latency" | "assumed";
};

const MIB = 1024 * 1024;

/** Memory = fixed + per request × in flight, fitted through the steps (engine: `memory_pods`). */
function memoryPods(steps: LimitStep[], c: CapacityInput, inFlightTotal: number) {
  const none = { pods: null, fixed: null, perRequest: null, note: null } as {
    pods: number | null;
    fixed: number | null;
    perRequest: number | null;
    note: string | null;
  };
  const limitMb = c.pod_memory_mb;
  if (!limitMb || !(limitMb > 0)) return none;
  const testedPods = Math.max(1, c.tested_pods || 1);
  const points = steps
    .filter((s) => !s.partial && s.apiMemoryBytes != null)
    .map((s) => [((s.rps ?? 0) * (s.latency.avg ?? 0)) / 1000 / testedPods, s.apiMemoryBytes as number]);
  if (points.length < 2)
    return { ...none, note: "fewer than two steps read the API's memory; add a monitor to size memory" };
  const n = points.length;
  const mx = points.reduce((a, p) => a + p[0], 0) / n;
  const my = points.reduce((a, p) => a + p[1], 0) / n;
  const sxx = points.reduce((a, p) => a + (p[0] - mx) ** 2, 0);
  const sxy = points.reduce((a, p) => a + (p[0] - mx) * (p[1] - my), 0);
  const perRequest = sxx > 0 ? Math.max(0, sxy / sxx) : 0;
  const fixed = Math.max(0, my - perRequest * mx);
  const budget = (limitMb * MIB * Math.min(100, Math.max(1, c.target_memory || 0))) / 100;
  if (fixed >= budget)
    return {
      pods: null,
      fixed,
      perRequest,
      note: "the fixed part alone is over the memory target, so more pods will not help; raise the memory limit or find what holds it (a cache, a leak)",
    };
  if (perRequest === 0)
    return { pods: 1, fixed, perRequest, note: "memory did not grow with the requests in flight, so it does not limit the pod count" };
  return { pods: Math.max(1, Math.ceil((inFlightTotal * perRequest) / (budget - fixed))), fixed, perRequest, note: null };
}

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
  const podsByCpu = Math.max(1, Math.ceil(designRps / usableRpsPerPod));
  const latencyMs = step.latency.avg ?? 0;
  const memory = memoryPods(steps, c, (designRps * latencyMs) / 1000);
  const podsForTraffic = Math.max(podsByCpu, memory.pods ?? 0);
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
    podsByCpu,
    podsByMemory: memory.pods,
    fixedMemoryBytes: memory.fixed,
    memoryPerRequestBytes: memory.perRequest,
    memoryNote: memory.note,
    basis,
  };
}
