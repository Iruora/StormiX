#!/usr/bin/env bash
# Runs the demo flow with Stormix and with k6 under the same load and prints
# a Markdown table. Needs the demo API on 127.0.0.1:3999, a release build of
# `stormix`, k6, jq and GNU time.
#   bench/compare.sh rate 500 20s   # open model: 500 iterations/s
#   bench/compare.sh vus 50 20s     # closed model: 50 users as fast as they can
set -euo pipefail
mode=${1:-rate} amount=${2:-500} duration=${3:-20s}
stormix=${STORMIX:-./target/release/stormix}
out=$(mktemp -d)

if [ "$mode" = rate ]; then
  stormix_args=(--rate "$amount" --duration "$duration")
  k6_env=(-e MODE=rate -e RATE="$amount" -e DURATION="$duration")
else
  stormix_args=(--vus "$amount" --duration "$duration")
  k6_env=(-e MODE=vus -e VUS="$amount" -e DURATION="$duration")
fi

/usr/bin/time -f "%M %P" -o "$out/stormix.time" \
  "$stormix" load examples/demo-api.stormix.yaml "${stormix_args[@]}" -q --out "$out/stormix.json" > /dev/null || true
/usr/bin/time -f "%M %P" -o "$out/k6.time" \
  k6 run -q --summary-trend-stats "avg,med,p(95),p(99),max" --summary-export "$out/k6.json" "${k6_env[@]}" bench/k6-demo.js > /dev/null || true

read -r s_mem s_cpu < "$out/stormix.time"
read -r k_mem k_cpu < "$out/k6.time"
s=$out/stormix.json k=$out/k6.json

echo "### Demo flow, $mode $amount for $duration"
echo
echo "| | Stormix | k6 |"
echo "|---|---|---|"
printf "| Requests | %s | %s |\n" "$(jq .summary.requests "$s")" "$(jq .metrics.http_reqs.count "$k")"
printf "| Requests/s | %.1f | %.1f |\n" "$(jq .summary.rps "$s")" "$(jq .metrics.http_reqs.rate "$k")"
printf "| Iterations dropped | %s | %s |\n" "$(jq .summary.droppedIterations "$s")" "$(jq '.metrics.dropped_iterations.count // 0' "$k")"
printf "| Latency p50 (ms) | %.2f | %.2f |\n" "$(jq .summary.latency.p50 "$s")" "$(jq '.metrics.http_req_duration.med' "$k")"
printf "| Latency p95 (ms) | %.2f | %.2f |\n" "$(jq .summary.latency.p95 "$s")" "$(jq '.metrics.http_req_duration["p(95)"]' "$k")"
printf "| Latency p99 (ms) | %.2f | %.2f |\n" "$(jq .summary.latency.p99 "$s")" "$(jq '.metrics.http_req_duration["p(99)"]' "$k")"
printf "| Failed requests | %s | %s |\n" "$(jq .summary.failedRequests "$s")" "$(jq '.metrics.http_req_failed.passes' "$k")"
printf "| Peak memory (MB) | %.0f | %.0f |\n" "$(echo "$s_mem / 1024" | bc -l)" "$(echo "$k_mem / 1024" | bc -l)"
printf "| CPU | %s | %s |\n" "$s_cpu" "$k_cpu"
echo
