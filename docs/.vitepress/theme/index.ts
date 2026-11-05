import DefaultTheme from 'vitepress/theme'
import type { Theme } from 'vitepress'
import { h } from 'vue'
import Shot from './components/Shot.vue'
import HomeShowcase from './components/HomeShowcase.vue'
import HomeHeroShot from './components/HomeHeroShot.vue'
import './style.css'

export default {
  extends: DefaultTheme,
  Layout: () =>
    h(DefaultTheme.Layout, null, {
      'home-hero-after': () => h(HomeHeroShot),
      'home-features-after': () => h(HomeShowcase),
    }),
  enhanceApp({ app }) {
    app.component('Shot', Shot)
  },
} satisfies Theme
