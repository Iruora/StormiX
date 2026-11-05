<script setup lang="ts">
// A screenshot of the app in the reader's theme. `name` is a file in
// public/shots/ without its -light/-dark suffix; `single` is for pictures that
// exist in one theme only.
import { withBase } from 'vitepress'
import { computed } from 'vue'

const props = defineProps<{ name: string; alt: string; caption?: string; single?: boolean; narrow?: boolean }>()
const light = computed(() => withBase(`/shots/${props.name}${props.single ? '' : '-light'}.webp`))
const dark = computed(() => withBase(`/shots/${props.name}${props.single ? '' : '-dark'}.webp`))
</script>

<template>
  <figure class="shot" :class="{ narrow }">
    <img v-if="single" :src="light" :alt="alt" loading="lazy" />
    <template v-else>
      <img class="only-light" :src="light" :alt="alt" loading="lazy" />
      <img class="only-dark" :src="dark" :alt="alt" loading="lazy" />
    </template>
    <figcaption v-if="caption">{{ caption }}</figcaption>
  </figure>
</template>
