import { defineConfig } from 'vitepress'

const repo = 'https://github.com/Iruora/StormiX'

// GitHub Pages serves a project site under /<repo>/. The deploy workflow sets
// DOCS_BASE from the repository name, so a fork or a rename still works.
const base = process.env.DOCS_BASE ?? '/'

// The reference pages keep their file names so links between them, and from
// the README, still work when the files are read on GitHub. On the site they
// get readable URLs.
const rewrites: Record<string, string> = {
  'GETTING_STARTED.md': 'guide/getting-started.md',
  'TEST_FILE.md': 'reference/test-file.md',
  'CLI.md': 'reference/cli.md',
  'COLLECTIONS.md': 'reference/collections.md',
  'COMPATIBILITY.md': 'reference/compatibility.md',
  'ROADMAP.md': 'roadmap.md',
}

// Points a link written for GitHub (`TEST_FILE.md#x`, or `docs/CLI.md` from
// CHANGELOG.md at the repository root) at the page's URL on the site.
function siteLink(href: string): string | undefined {
  const m = /^(?:\.\/|docs\/)?([A-Z_]+\.md)(#.*)?$/.exec(href)
  const target = m && rewrites[m[1]]
  return target ? '/' + target.replace(/\.md$/, '') + (m[2] ?? '') : undefined
}

export default defineConfig({
  title: 'Stormix',
  description:
    'Stress test APIs and run collection flows, from a desktop app or the command line. A free, open source alternative to k6 with a GUI.',
  base,
  lang: 'en-US',
  cleanUrls: true,
  lastUpdated: true,

  rewrites,

  // Links out of docs/ (README, examples, the changelog's own links) go to GitHub.
  ignoreDeadLinks: [/^\.\.\//],

  head: [
    ['link', { rel: 'icon', type: 'image/png', href: `${base}logo.png` }],
    ['meta', { name: 'theme-color', content: '#4f46e5' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:title', content: 'Stormix: API load tests and regression runs' }],
    [
      'meta',
      {
        property: 'og:description',
        content:
          'A free, open source desktop app and CLI to stress test APIs and replay Postman and Bruno collections.',
      },
    ],
    ['meta', { property: 'og:image', content: `${base}shots/app-last-run-dark.webp` }],
  ],

  markdown: {
    config(md) {
      // `{{variable}}` is everywhere in these docs. Keep Vue from reading it
      // as a template expression inside inline code.
      const codeInline = md.renderer.rules.code_inline!
      md.renderer.rules.code_inline = (tokens, idx, options, env, self) =>
        codeInline(tokens, idx, options, env, self).replace(/^<code/, '<code v-pre')

      md.core.ruler.push('stormix_doc_links', (state) => {
        for (const block of state.tokens) {
          for (const t of block.children ?? []) {
            if (t.type !== 'link_open') continue
            const link = siteLink(t.attrGet('href') ?? '')
            if (link) t.attrSet('href', link)
          }
        }
      })
    },
  },

  themeConfig: {
    logo: '/logo.png',
    nav: [
      { text: 'Guide', link: '/guide/introduction', activeMatch: '/guide/' },
      { text: 'Desktop app', link: '/app/tour', activeMatch: '/app/' },
      { text: 'Reference', link: '/reference/test-file', activeMatch: '/reference/' },
      { text: 'Changelog', link: '/changelog' },
      { text: 'Download', link: `${repo}/releases/latest` },
    ],

    sidebar: [
      {
        text: 'Guide',
        items: [
          { text: 'What is Stormix?', link: '/guide/introduction' },
          { text: 'Getting started', link: '/guide/getting-started' },
          { text: 'Load testing', link: '/guide/load-testing' },
          { text: 'Running in CI', link: '/guide/ci' },
        ],
      },
      {
        text: 'Desktop app',
        items: [
          { text: 'A tour of the app', link: '/app/tour' },
          { text: 'Editing steps and YAML', link: '/app/editing' },
          { text: 'The Flow view', link: '/app/flow' },
          { text: 'Variables and scripts', link: '/app/variables-and-scripts' },
          { text: 'Environments and secrets', link: '/app/environments' },
          { text: 'Workspaces', link: '/app/workspaces' },
          { text: 'Load tests in the app', link: '/app/load-tests' },
          { text: 'Runs, diffs and reports', link: '/app/runs' },
          { text: 'Debugging a failed request', link: '/app/debugging' },
        ],
      },
      {
        text: 'Features',
        items: [
          { text: 'GraphQL', link: '/features/graphql' },
          { text: 'Response cache', link: '/features/cache' },
          { text: 'Tracing in Dynatrace and APM tools', link: '/features/tracing' },
          { text: 'API CPU and memory', link: '/features/api-resources' },
          { text: 'Proxies', link: '/features/proxies' },
          { text: 'Import and export collections', link: '/features/import-export' },
        ],
      },
      {
        text: 'Reference',
        items: [
          { text: 'Test files', link: '/reference/test-file' },
          { text: 'CLI', link: '/reference/cli' },
          { text: 'Collections', link: '/reference/collections' },
          { text: 'Compatibility', link: '/reference/compatibility' },
        ],
      },
      {
        text: 'Project',
        items: [
          { text: 'Changelog', link: '/changelog' },
          { text: 'Roadmap', link: '/roadmap' },
        ],
      },
    ],

    socialLinks: [{ icon: 'github', link: repo }],
    editLink: {
      pattern: `${repo}/edit/main/docs/:path`,
      text: 'Edit this page on GitHub',
    },
    search: { provider: 'local' },
    outline: { level: [2, 3] },
    footer: {
      message: 'Released under the MIT or Apache 2.0 license, at your option.',
      copyright: 'Stormix contributors',
    },
  },
})
