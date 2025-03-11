<script setup lang="ts">
const props = defineProps(['label', 'route', 'customClasses'])
</script>

<template>
  <router-link
    :to="props.route"
    class="w-full md:w-auto inline-flex items-center justify-center gap-2.5 py-4 px-2 sm:px-10 text-center font-medium hover:bg-opacity-90 lg:px-8 xl:px-10"
    :class="props.customClasses"
  >
    <slot></slot>
    {{ props.label }}
  </router-link>
</template>
