<script setup lang="ts">
import { withBase } from 'vitepress'
</script>

<template>
  <div class="hero-shot">
    <div class="window">
      <div class="window-bar"><span></span><span></span><span></span></div>
      <img class="only-light" :src="withBase('/shots/app-last-run-light.webp')" alt="The Stormix desktop app after a passing run" />
      <img class="only-dark" :src="withBase('/shots/app-last-run-dark.webp')" alt="The Stormix desktop app after a passing run" />
    </div>
  </div>
</template>
