---
layout: home
title: Stormix
titleTemplate: API load tests and regression runs, with a GUI

hero:
  name: Stormix
  text: Stress test your APIs. Replay your collections.
  tagline: A free, open source desktop app and CLI that runs your Postman and Bruno flows as regression tests, then puts them under load. Like k6, with a GUI.
  image:
    src: /logo.png
    alt: Stormix
  actions:
    - theme: brand
      text: Download
      link: https://github.com/Iruora/StormiX/releases/latest
    - theme: alt
      text: Get started
      link: /guide/getting-started
    - theme: alt
      text: View on GitHub
      link: https://github.com/Iruora/StormiX

features:
  - icon: ⚡
    title: Load and stress tests
    details: Constant-rate, ramping and virtual-user models, exact p50 to p99 latency from HDR histograms, a live chart, and thresholds like p95 < 300ms that pass or fail the run.
    link: /guide/load-testing
    linkText: Load testing
  - icon: 🔁
    title: Regression runs from your collections
    details: Postman and Bruno collections run as they are. Chained requests, extracted variables, assertions, and their pre-request and test scripts.
    link: /reference/collections
    linkText: Collections
  - icon: 🧭
    title: A visual flow editor
    details: Edit steps in a form, in a validated YAML editor, or as a flow diagram where you drag, turn off and set aside steps.
    link: /app/flow
    linkText: The Flow view
  - icon: 📈
    title: Run history and diffs
    details: Every run is kept. Compare two to see which steps broke or were fixed, or how throughput and latency moved between load tests.
    link: /app/runs
    linkText: Runs and diffs
  - icon: 🩺
    title: Debug failures fast
    details: Details shows the request as sent and the response as received. Diagnose walks the proxy, DNS, TLS and response to find who refused it.
    link: /app/debugging
    linkText: Debugging
  - icon: 🛠️
    title: Same files in CI
    details: The stormix CLI runs test files and collections headless, with exit codes, JUnit, JSON and self-contained HTML reports.
    link: /guide/ci
    linkText: Running in CI
---
