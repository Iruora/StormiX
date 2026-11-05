<script setup lang="ts">
import { withBase } from 'vitepress'
import { ref, computed } from 'vue'

const repo = 'https://github.com/Iruora/StormiX'

const tabs = [
  {
    id: 'flow',
    label: 'Flow view',
    title: 'See the whole flow, change it by dragging',
    text: 'Every step as a card with its last result. Turn steps on and off, drag them to reorder or set them aside, and get a warning when a step now needs a variable nothing sets before it.',
    shot: 'flow-after-run',
    link: '/app/flow',
  },
  {
    id: 'load',
    label: 'Load tests',
    title: 'Pick the endpoints, set the load, watch it live',
    text: 'Choose which steps go under load, a constant rate, ramping stages or virtual users, and thresholds such as p95 < 300ms. Results sort and filter per step, with the most common failures one click from their details.',
    shot: 'load-sorted',
    link: '/app/load-tests',
  },
  {
    id: 'vars',
    label: 'Variables',
    title: 'Every variable explains itself',
    text: 'Hover a {{variable}} to see its value and where it comes from. Edit it in place for this session, or save it to the environment file.',
    shot: 'var-edit',
    link: '/app/variables-and-scripts',
  },
  {
    id: 'scripts',
    label: 'Scripts',
    title: 'Your Postman and Bruno scripts, still running',
    text: 'Pre-request and post-response JavaScript runs in a sandbox with the pm.* and bru.* APIs. Edit it in a code editor that completes bru, req, res and your own variable names.',
    shot: 'scripts-complete',
    link: '/app/variables-and-scripts#scripts',
  },
  {
    id: 'diagnose',
    label: 'Diagnose',
    title: 'Find out who refused the request',
    text: 'Diagnose sends a failed request again and checks each stage: proxy, DNS, connection, TLS certificate and response, then tells you whether the API, a firewall, a CDN or a corporate proxy answered.',
    shot: 'diagnose',
    link: '/app/debugging',
  },
  {
    id: 'env',
    label: 'Environments',
    title: 'Environments from anywhere',
    text: 'Bring environments from Bruno, Postman, YAML or JSON. Change a value for this session or for good, and keep secrets in memory only.',
    shot: 'env-session',
    link: '/app/environments',
  },
]

const active = ref(tabs[0].id)
const current = computed(() => tabs.find((t) => t.id === active.value)!)
</script>

<template>
  <div class="home-more">
    <!-- One flow, two jobs -->
    <section class="home-section">
      <p class="eyebrow">One engine, two jobs</p>
      <h2>The flow that passes your regression run is your load test</h2>
      <div class="two-jobs">
        <div class="job">
          <div class="job-num">1</div>
          <h3>Regression runs</h3>
          <p>
            Import a Postman or Bruno collection as it is. Requests are chained, variables pass between steps, and
            assertions and scripts run. <strong>Run once</strong> shows every check that passed or failed and why.
          </p>
        </div>
        <div class="job-arrow" aria-hidden="true">→</div>
        <div class="job">
          <div class="job-num">2</div>
          <h3>Load and stress tests</h3>
          <p>
            Replay the same flow from hundreds of users with constant-rate, ramping or virtual-user models. Get exact
            p50 to p99 latency, errors and throughput, and thresholds that fail the build.
          </p>
        </div>
        <div class="job-arrow" aria-hidden="true">→</div>
        <div class="job">
          <div class="job-num">3</div>
          <h3>In CI, unchanged</h3>
          <p>
            The <code>stormix</code> CLI runs the same files in any pipeline and writes JUnit, JSON and self-contained
            HTML reports.
          </p>
        </div>
      </div>
    </section>

    <!-- Showcase -->
    <section class="home-section">
      <p class="eyebrow">A tour</p>
      <h2>Built for the people who test APIs every day</h2>
      <div class="showcase">
        <div class="showcase-tabs" role="tablist">
          <button
            v-for="t in tabs"
            :key="t.id"
            role="tab"
            :aria-selected="active === t.id"
            :class="{ active: active === t.id }"
            @click="active = t.id"
          >
            {{ t.label }}
          </button>
        </div>
        <div class="showcase-body">
          <div class="showcase-text">
            <h3>{{ current.title }}</h3>
            <p>{{ current.text }}</p>
            <a class="more-link" :href="withBase(current.link)">Read how it works →</a>
          </div>
          <div class="showcase-shot window">
            <div class="window-bar"><span></span><span></span><span></span></div>
            <img class="only-light" :src="withBase(`/shots/${current.shot}-light.webp`)" :alt="current.title" />
            <img class="only-dark" :src="withBase(`/shots/${current.shot}-dark.webp`)" :alt="current.title" />
          </div>
        </div>
      </div>
    </section>

    <!-- Numbers -->
    <section class="home-section">
      <p class="eyebrow">Fast and light</p>
      <h2>A Rust engine that keeps up with your API</h2>
      <div class="stats">
        <div class="stat">
          <div class="stat-value">42.7k</div>
          <div class="stat-label">requests a second at 50 virtual users, where k6 sent 22.3k on the same machine</div>
        </div>
        <div class="stat">
          <div class="stat-value">13 MB</div>
          <div class="stat-label">of memory for that test, against k6's 153 MB</div>
        </div>
        <div class="stat">
          <div class="stat-value">3 digits</div>
          <div class="stat-label">of precision on every percentile, from HDR histograms, at any volume</div>
        </div>
      </div>
      <p class="fine">
        Measured with <a :href="`${repo}/blob/main/bench/compare.sh`">bench/compare.sh</a>, which runs both tools
        against the same demo API in CI.
      </p>
    </section>

    <!-- CLI -->
    <section class="home-section cli-section">
      <div class="cli-text">
        <p class="eyebrow">For CI</p>
        <h2>Gate your pipeline on latency</h2>
        <p>
          <code>stormix run</code> exits with 1 when a check fails and <code>stormix load</code> when a threshold
          fails. Point either at a test file or straight at a collection.
        </p>
        <a class="more-link" :href="withBase('/guide/ci')">Set it up in GitHub Actions, GitLab or Jenkins →</a>
      </div>
      <pre class="terminal"><code><span class="p">$</span> stormix load checkout.stormix.yaml --threshold "p95 &lt; 300ms"

  requests    3050 (152.2/s), 0 failed (0.00%)
  checks      10675/10675 passed (100.00%)
  latency     p50 0.3 ms  p95 0.5 ms  p99 0.9 ms

  thresholds
    <span class="ok">✓</span> p95 &lt; 300ms          0.5 ms
    <span class="ok">✓</span> errors &lt; 1%          0.00%</code></pre>
    </section>

    <!-- Works with -->
    <section class="home-section">
      <p class="eyebrow">Bring what you have</p>
      <h2>Works with your collections, both ways</h2>
      <div class="formats">
        <div class="format"><strong>Postman</strong><span>v2.0 and v2.1 collections and environments, in and out</span></div>
        <div class="format"><strong>Bruno</strong><span>.bru folders and environments, scripts and assertions</span></div>
        <div class="format"><strong>OpenCollection</strong><span>Bruno's YAML format since v3, folder or bundled file, in and out</span></div>
        <div class="format"><strong>GraphQL</strong><span>typed variables and a failed check on GraphQL errors</span></div>
        <div class="format"><strong>W3C trace context</strong><span>find any slow request in Dynatrace, Datadog or Jaeger</span></div>
        <div class="format"><strong>JUnit, JSON, HTML</strong><span>reports for CI test tabs, tickets and email</span></div>
      </div>
    </section>

    <!-- CTA -->
    <section class="home-section cta">
      <h2>Free and open source. Ready on macOS, Windows and Linux.</h2>
      <p>No account, no cloud, no telemetry. MIT or Apache 2.0, your choice.</p>
      <div class="cta-actions">
        <a class="cta-button brand" :href="`${repo}/releases/latest`">Download Stormix</a>
        <a class="cta-button alt" :href="withBase('/guide/getting-started')">Read the getting started guide</a>
      </div>
    </section>
  </div>
</template>
