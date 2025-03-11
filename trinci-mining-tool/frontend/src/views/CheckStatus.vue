<script setup lang="ts">
import { useNodeStore } from '@/stores/node';
import { onMounted } from 'vue';
import { useRouter } from 'vue-router';

const { initMiningTool } = useNodeStore();
const router = useRouter();

onMounted(() => {
  return initMiningTool(false).then(() => {
    router.replace({name: 'home'})
  }).catch(() => {
    router.replace({name: 'step1setup'})
  }).finally(() => {}); // TODO: Scarpa allaccia allaccia scarpa LOOADERRR
})
</script>
<template></template>