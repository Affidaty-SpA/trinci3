<script setup lang="ts">
import { useSidebarStore } from '@/stores/sidebar'
import { useRoute } from 'vue-router'
import ProgressOne from '@/components/Progress/ProgressOne.vue'
import { useNodeStore } from "@/stores/node"
import { toRefs } from 'vue'
import { chop } from '@/utils/utils'

const sidebarStore = useSidebarStore()
const props = defineProps(['item', 'index'])
const { item } = toRefs(props)
const currentPage = useRoute().name
const { setCurrentNode } = useNodeStore()

const emit = defineEmits(['itemSelected'])

interface SidebarItem {
  label: string
}

const handleItemClick = () => {
  if (!item!.value.node_disabled) {
    setCurrentNode(item!.value.node_id);
    const pageName = sidebarStore.page === item!.value.label ? '' : item!.value.label
    sidebarStore.page = pageName

    emit('itemSelected', props.item.node_id)
    /* if (props.item.children) {
      return props.item.children.some((child: SidebarItem) => sidebarStore.selected === child.label)
    } */
  }
}
</script>
<template>
  <div>
    <router-link :to="item.route"
      class="relative group mr-3.5"
      @click.prevent="handleItemClick" :class="{
      'bg-graydark dark:bg-meta-4': sidebarStore.page === item.label, [item.node_classes]: true
    }">

      <!-- Active state -->
      <div v-if="item.node_selected" class="absolute h-full top-0 -right-7 flex items-center">
        <img class="z-999 h-[58px]" src="@/assets/images/icons/active-node.svg" alt="Active Node" />
      </div>

      <!-- Alignment State -->
      <div v-if="item.node_alignment" class="absolute left-0 top-0 w-full h-full z-1 rounded-md bg-mining-gray/90 flex ">
        <div class="w-full mt-5">
          <p class="text-center font-bold">Alignment in progress</p>
          <ProgressOne />
        </div>
      </div>
      <!-- Disabled State -->
      <div v-if="item.node_disabled" class="absolute left-0 top-0 w-full h-full z-1 rounded-md bg-mining-gray/80 flex ">
      </div>
      <div class="flex items-center gap-x-4 mr-24">
        <div class="flex space-x-1.5 node-indicator">
          <div v-if="item.node_indicator_1" class="h-3 w-3 rounded-full"></div>
          <div v-else class="h-3 w-3 rounded-full opacity-30"></div>
          <div v-if="item.node_indicator_2" class="h-3 w-3 rounded-full"></div>
          <div v-else class="h-3 w-3 rounded-full opacity-30"></div>
          <div v-if="item.node_indicator_3" class="h-3 w-3 rounded-full"></div>
          <div v-else class="h-3 w-3 rounded-full opacity-30"></div>
        </div>
        <div class="truncate node-id">
          {{ chop(item.node_id, 5, 5, '...') }}
        </div>
        <span
          class="node-type absolute right-4 block rounded-md  border py-1 px-4 text-xs font-semibold uppercase">
          {{ item.node_classes }}
        </span>
      </div>
      <div class="grid grid-cols-12 divide-x divide-mining-gray-3">
        <div class="col-span-6 flex  gap-x-1">
          <img :src="'/src/assets/images/tokens/img-' + item.node_classes + '-stake.svg'" alt="Player Stake" class="h-4.5 w-4.5">
          <span class="node-value flex text-sm"><span>Stake:&nbsp;</span> <span class="font-bold">{{ item.node_value_stake_a }}</span>&nbsp;/&nbsp;{{ item.node_value_stake_b }}</span>
        </div>
        <div class="col-span-6 flex  gap-x-1 pl-4">
          <img :src="'/src/assets/images/tokens/img-' + item.node_classes + '-bitbel.svg'" alt="Player Stake" class="h-4.5 w-4.5">
          <span class="node-value text-sm">{{ item.node_value_bitbel }}</span>
        </div>
      </div>
    </router-link>
  </div>
</template>

<style scoped>
@import 'tailwindcss/components';

/* Class for node players */
.player {
    @apply  text-bodydark1 bg-mining-gray-2 hover:bg-mining-gray-2/60  relative flex flex-col gap-3.5 py-3 px-4 font-medium duration-300 ease-in-out  rounded-[6px]
}
.player .node-indicator div{
  @apply bg-mining-green
}
.player .node-type {
  @apply text-mining-green border-mining-green
}

/* Class for node npc */
.npc {
    @apply  text-bodydark1 ring-mining-blue hover:bg-mining-gray-2/60 bg-transparent ring-1  relative flex flex-col gap-3.5 py-3 px-4 font-medium duration-300 ease-in-out  rounded-[6px]
}
.npc .node-indicator div{
  @apply bg-mining-blue
}
.npc .node-type {
  @apply text-mining-blue border-mining-blue
}
.npc .node-id {
  @apply text-mining-blue
}
.npc .node-value {
  @apply text-mining-blue
}
</style>
