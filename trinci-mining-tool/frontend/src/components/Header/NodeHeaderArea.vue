<script setup lang="ts">
import { useSidebarStore } from '@/stores/sidebar'
import DropdownUser from './DropdownUser.vue'
import { useRoute } from 'vue-router';
import { storeToRefs } from 'pinia';
import { useNodeStore } from '@/stores/node';
import { computed, onMounted, ref, watchPostEffect } from 'vue';
import { EphemeralTx } from '@/utils/trinci';
import { useToast } from 'vue-toastification';

const { isSidebarOpen, toggleSidebar } = useSidebarStore();
const route = useRoute();
const { lastBlockHeight, nodeData, currentAccountId, otherNodes } = storeToRefs(useNodeStore());
const { updateNodeStatus } = useNodeStore();
const accountId = currentAccountId.value;
const toast = useToast();

const isActive = ref<boolean>(false)
const isPlayer = ref<boolean>(false)

const toggleNodeStatus = async () => {
  if (isActive.value) {
    return new EphemeralTx(currentAccountId.value, "stop_mining" , new Uint8Array([92])).startAndStopMining().then((response) => {
      isActive.value = false;
      updateNodeStatus(isActive.value, currentAccountId.value);
      toast.success('Mining task has been stopped')
    }).catch((e) => {
      toast.error(`Something went wrong err: ${e}`)
    });
  } else {
    return new EphemeralTx(currentAccountId.value, "start_mining" , new Uint8Array([92])).startAndStopMining().then((response) => {
      isActive.value = true;
      updateNodeStatus(isActive.value, currentAccountId.value);
      toast.success('Mining task has been started')
    }).catch((e) => {
      toast.error(`Something went wrong err: ${e}`)
    });
  }
}

const refreshData = () => {
  isActive.value = nodeData?.value?.status || false;
  isPlayer.value = otherNodes.value.some(n => n.account_id === accountId);
  // isActive.value = activeMiningNodes.value.find((i) => i[0] === accountId) ? activeMiningNodes.value.find((i) => i[0] === accountId)![1] : false;
  // isPlayer.value = nodeData?.value && nodeData.value?.players?.some((player: any) => nodeData?.value && nodeData.value.account_id === player.account_id) ? true : false;
}

watchPostEffect(() => {
  if (nodeData?.value) {
    refreshData()
  }
})

onMounted(() => {
  refreshData();
})
</script>

<template>
  <header
    class="sticky top-0 z-999 flex w-full bg-white drop-shadow-1 dark:bg-boxdark dark:drop-shadow-none"
  >
    <div class="flex flex-grow justify-start lg:justify-end py-4 px-4 shadow-2 md:px-6 2xl:px-11">
      <div class="flex items-center gap-2 sm:gap-4 lg:hidden">
        <!-- Hamburger Toggle BTN -->
        <button
          class="z-99999 block rounded-sm border border-stroke bg-white p-1.5 shadow-sm dark:border-strokedark dark:bg-boxdark lg:hidden"
          @click="
            () => {
              console.log('Toggling Sidebar')
              toggleSidebar()
            }
          "
        >
          <span class="relative block h-5.5 w-5.5 cursor-pointer">
            <span class="du-block absolute right-0 h-full w-full">
              <span
                class="relative top-0 left-0 my-1 block h-0.5 w-0 rounded-sm bg-black delay-[0] duration-200 ease-in-out dark:bg-white"
                :class="{ '!w-full delay-300': !isSidebarOpen }"
              ></span>
              <span
                class="relative top-0 left-0 my-1 block h-0.5 w-0 rounded-sm bg-black delay-150 duration-200 ease-in-out dark:bg-white"
                :class="{ '!w-full delay-400': !isSidebarOpen }"
              ></span>
              <span
                class="relative top-0 left-0 my-1 block h-0.5 w-0 rounded-sm bg-black delay-200 duration-200 ease-in-out dark:bg-white"
                :class="{ '!w-full delay-500': !isSidebarOpen }"
              ></span>
            </span>
            <span class="du-block absolute right-0 h-full w-full rotate-45">
              <span
                class="absolute left-2.5 top-0 block h-full w-0.5 rounded-sm bg-black delay-300 duration-200 ease-in-out dark:bg-white"
                :class="{ '!h-0 delay-[0]': !isSidebarOpen }"
              ></span>
              <span
                class="delay-400 absolute left-0 top-2.5 block h-0.5 w-full rounded-sm bg-black duration-200 ease-in-out dark:bg-white"
                :class="{ '!h-0 dealy-200': !isSidebarOpen }"
              ></span>
            </span>
          </span>
        </button>
        <!-- Hamburger Toggle BTN -->
        <router-link class="hidden flex-shrink-0 lg:hidden" to="/">
          <img src="@/assets/images/logo/logo-icon.svg" alt="Logo" />
        </router-link>
      </div>


      <div class="flex items-center gap-3 2xsm:gap-7 justify-between lg:justify-start w-full lg:w-auto">
        <ul class="lg:flex items-center gap-2 2xsm:gap-4">
          <li class="flex items-center gap-3 lg:border-r-2 border-mining-gray-2 pr-20 pl-5 lg:pl-0">
            <div class="relative">
              <img src="@/assets/images/icons/mt-white.svg"/>
              <div :class="{
                '*:bullet absolute top-0 -right-[3px] w-2 h-2 rounded-full': true,
                'bg-mining-green animation-pulse-green': isActive,
                'bg-red': !isActive
                }"></div>
              <!-- To stop animation pulse: delete class:animation-pulse-green and replace bg-mining-green to bg-red -->
            </div>
            <div>
              <p class="text-sm text-mining-gray-3">Node mining status</p>
              <button @click="toggleNodeStatus" class="rounded bg-mining-blue px-2 text-xs font-bold text-form-input cursor-pointer hover:bg-red hover:text-white">{{ isActive ? 'Stop' : 'Start' }}</button>
            </div>
          </li>
          <li class="hidden lg:flex items-center gap-3 border-r-2 border-mining-gray-2 pr-20 relative">

            <div class="relative">
              <img src="@/assets/images/icons/node.svg" class="mt-2"/>
              <div :class="{
                '*:bullet absolute top-0 -right-[3px] w-2 h-2 rounded-full': true,
                'bg-mining-green animation-pulse-green': isActive,
                'bg-red': !isActive
                }"></div>
            </div>
            <div class="node-status">
              <p class="text-sm text-mining-gray-3">Nodes status</p>
             On <span>|</span>
             Height: {{ nodeData?.health?.block_height || 0 }}
            </div>
            <div class="node-type ml-3 rounded-md border border-mining-green  text-mining-green py-1 px-4 text-xs font-semibold uppercase">{{ isPlayer ? 'player' : 'npc' }}</div>
          </li>
          <li class="hidden lg:flex  items-center gap-3 border-r-2 border-mining-gray-2 pr-20">
            <div class="relative">
              <img src="@/assets/images/icons/chain-white.svg"/>
            </div>
            <div>
              <p class="text-sm text-mining-gray-3">Chain</p>
              <div class="flex node-status">
                Roselle net
                <button class="pl-5 pr-2"><img src="@/assets/images/icons/icon-copy.svg"/></button>
                <span>|</span>
                Height: {{ lastBlockHeight }}
              </div>

            </div>
          </li>
        </ul>

        <!-- User Area -->
        <DropdownUser />
        <!-- User Area -->
      </div>
    </div>
  </header>
</template>

<style scoped>
.node-status strong{
@apply text-mining-green inline-block pr-1
}
.node-status span{
@apply text-mining-gray-3 inline-block px-2
}

</style>
