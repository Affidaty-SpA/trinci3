<script setup lang="ts">
import { RouterView } from 'vue-router'
import TransitionFade from "./components/TransitionFade.vue";
import Loader from './components/Loader.vue';
import { storeToRefs } from 'pinia';
import { useRootStore } from './stores/index.store';

const { loaderMessage, showLoader } = storeToRefs(useRootStore());
</script>

<template>
  <RouterView />
  <TransitionFade>
    <Loader :messages="loaderMessage" v-if="showLoader" />
  </TransitionFade>
</template>

<style scoped></style>
