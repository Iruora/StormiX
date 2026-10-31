// The same flow as examples/demo-api.stormix.yaml, for comparing Stormix
// with k6 on one machine. bench/compare.sh runs both.
import http from "k6/http";
import { check } from "k6";

const BASE = "http://127.0.0.1:3999";
const MODE = __ENV.MODE || "rate";
const RATE = Number(__ENV.RATE || 500);
const VUS = Number(__ENV.VUS || 50);
const DURATION = __ENV.DURATION || "20s";

export const options = {
  discardResponseBodies: false,
  scenarios: {
    demo:
      MODE === "rate"
        ? { executor: "constant-arrival-rate", rate: RATE, timeUnit: "1s", duration: DURATION, preAllocatedVUs: 200, maxVUs: 1000 }
        : { executor: "constant-vus", vus: VUS, duration: DURATION },
  },
};

export default function () {
  const login = http.post(`${BASE}/login`, JSON.stringify({ user: "demo", password: "secret" }), {
    headers: { "Content-Type": "application/json" },
  });
  check(login, { "status is 200": (r) => r.status === 200 });
  const token = login.json("token");
  const me = http.get(`${BASE}/me`, { headers: { Authorization: `Bearer ${token}` } });
  check(me, {
    "status is 200": (r) => r.status === 200,
    "user is demo": (r) => r.json("user") === "demo",
  });
}
