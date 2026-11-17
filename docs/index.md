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
    details: Edit steps in a form, in a validated YAML editor, or as a flow diagram where you drag, turn off and set aside steps, split them into parallel branches, and choose whether a failed step stops the run.
    link: /app/flow
    linkText: The Flow view
  - icon: 📈
    title: Run history and diffs
    details: Every run is kept. Compare two to see which steps broke or were fixed, or how throughput and latency moved between load tests.
    link: /app/runs
    linkText: Runs and diffs
  - icon: 🩺
    title: Debug failures fast
    details: Details shows the request as sent and the response as received. Diagnose walks the proxy, DNS, TLS and response to find who refused it. Copy any step as a cURL command, no run needed.
    link: /app/debugging
    linkText: Debugging
  - icon: 🛠️
    title: Same files in CI
    details: The stormix CLI runs test files and collections headless, with exit codes, JUnit, JSON and self-contained HTML reports.
    link: /guide/ci
    linkText: Running in CI
  - icon: ✨
    title: Explain results with your own AI
    details: Ask Claude Code or GitHub Copilot CLI, signed in on your computer, why a step failed or what a load test says. No API key, secrets hidden, and you see exactly what is sent.
    link: /features/ai
    linkText: Explain with AI
  - icon: 🎲
    title: Test data for load tests
    details: Give each iteration or virtual user its own user ID, product or search term, from a list, rows or a CSV file, picked at random, in order or once each.
    link: /features/test-data
    linkText: Test data
  - icon: 📦
    title: Find the limit, size the pods
    details: Step the rate up until the API breaks, watch its CPU, memory and health, fail the test when it uses too much or goes down, and get how many pods your production peak needs.
    link: /features/capacity
    linkText: Capacity and pods
---
