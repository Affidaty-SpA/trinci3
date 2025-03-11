<script setup lang="ts">
import NodeLayout from '@/layouts/NodeLayout.vue';
import ChartMiningDetail from '@/components/Charts/ChartMiningDetail.vue';
import ChartUsage from '@/components/Charts/ChartUsage.vue';
import NodeCards from '@/components/Cards/NodeCards.vue';
import ConfigVisa from '@/components/Nodes/ConfigVisa.vue';
import { useRoute, useRouter } from 'vue-router';
import { onMounted, ref } from 'vue';
import { useNodeStore } from '@/stores/node';
import { useSidebarStore } from '@/stores/sidebar';
import { storeToRefs } from 'pinia';

const route = useRoute();
const accountId = ref(route.params.accountId as string);
const { nodeData } = storeToRefs(useNodeStore());
const { menuRefs } = storeToRefs(useSidebarStore())
const router = useRouter();

onMounted(() => {
  if (nodeData?.value && Object.keys(nodeData.value).length === 0) {
    menuRefs.value = []
    router.replace({name: "home"});
  }
})

</script>

<template>
  <NodeLayout>
    <h1 class="page-title">Node detail</h1>
    <p class="page-subtitle">{{ accountId }}</p>
    <!--4 boxes-->
    <NodeCards :account-id="accountId" :key="accountId" />

    <!--end 4 boxes-->

    <!--Charts area-->
    <div class="mt-7.5 grid grid-cols-12 gap-4 md:gap-6 2xl:gap-7.5">

      <div class="col-span-12 xl:col-span-3">
        <ChartUsage :account-id="accountId" />
      </div>
      <div class="col-span-12 xl:col-span-6">
        <ChartMiningDetail />
      </div>
      <div class="col-span-12 xl:col-span-3">
        <ConfigVisa />
      </div>

    </div>
    <!--end Charts area-->
  </NodeLayout>
</template>
