<script setup lang="ts">
import { watchPostEffect, onMounted, ref } from 'vue';
import { InformationCircleIcon } from '@heroicons/vue/24/outline';
import { useNodeStore } from "@/stores/node";
import { storeToRefs } from 'pinia';
import { useRouter } from 'vue-router';
import { useToast } from 'vue-toastification';

export interface IDataList {
  title: string,
  button1?: any,
  button2?: any,
  total?: number,
  totalgeneral?: number,
  percent?: number
}

const last = <T>(a: T[]) => {
  return a[a.length - 1];
}

const { genericNodesData, allNodesData, otherNodes } = storeToRefs(useNodeStore());
const dataList = ref<IDataList[]>([])
const router = useRouter();
const toast = useToast();

const refreshData = () => {
  const spreadedData = [...allNodesData.value.values()];

  const tcoinTotal = spreadedData.reduce((acc, curr) => {
      acc += last<any>(curr.stats.tcoin || [{ time: 0, units_in_stake: 0 }]).units_in_stake;
      return acc
    }, 0)

  const tcoinStake = spreadedData.reduce((acc, curr) => {
      acc += last<any>(curr.stats.tcoin || [{ time: 0, total: 0 }]).total;
      return acc
    }, 0)

  const bitbelTotal = spreadedData.reduce((acc, curr) => {
      acc += last<any>(curr.stats.bitbel || [{ time: 0, units: 0 }]).units;
      return acc
    }, 0)

  dataList.value = [
    {
      title: 'Nodes',
      total: allNodesData.value.size,
      button1: {
        title: "Add new Node",
        handler: () => {
          toast.success("You'll be redirect to setup page", {timeout: 3000})
          setTimeout(() => {
            router.push({name: 'step1setup'})
          }, 3000)
        }
      }
    },
    {
      title: 'My players',
      total: allNodesData.value.size,
      totalgeneral: otherNodes.value.length,
      percent: parseInt((otherNodes.value.length !=0 ? ((allNodesData.value.size) / otherNodes.value.length) * 100 : 100).toFixed(0))
    },
    {
      title: 'Bitbel',
      total: bitbelTotal,
      button1: {
        title: "Buy",
        handler: () => { toast.success('To buy Bitbel go on fuel-machine') }
      },
      button2: {
        title: "Sell",
        handler: () => {toast.success('To sell Bitbel go on fuel-machine')}
      }
    },
    {
      title: 'Tcoin stake',
      total: tcoinTotal,
      totalgeneral: tcoinStake,
      percent: parseInt(((tcoinTotal / tcoinStake) * 100).toFixed(0))
    }
  ]
}

watchPostEffect(() => {
  if (allNodesData.value) {
    refreshData();
  }
})

onMounted(() => {
  refreshData();
})

</script>
<template>
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 md:gap-6 xl:grid-cols-4 2xl:gap-7.5">
      <template v-for="(item, index) in dataList" :key="index">
        <div
          class="relative rounded-xl border border-stroke bg-white p-4 shadow-default dark:border-strokedark dark:bg-boxdark md:p-6 xl:p-7.5"
        >
        <button class="absolute right-3 top-3">
          <InformationCircleIcon class=" text-mining-blue hover:text-mining-green w-6 h-6 "/>
        </button>
          <div class="flex items-end justify-between">
            <div>
              <h3 class="mb-4 text-5xl font-bold text-white">
                {{ item.total }}
                <span v-if="item.totalgeneral" class=" text-xl font-normal">
                  /{{ item.totalgeneral }}
                </span>
              </h3>
              <p class="font-medium">{{ item.title }}</p>
              <div class="mt-2 flex items-center gap-2">
                <button v-if="item.button1" @click="() => item.button1.handler()"
                  class="flex items-center gap-1 rounded-md p-1 text-md font-bold text-form-input bg-mining-blue hover:bg-mining-green px-5"
                > <span>{{ item.button1.title }}</span>
                </button>
                <button v-if="item.button2" @click="() => item.button2.handler()"
                  class="flex items-center gap-1 rounded-md p-1 text-md font-bold text-form-input bg-mining-blue hover:bg-mining-green px-5"
                > <span>{{ item.button2.title }}</span>
                </button>
              </div>
            </div>

            <div v-if="item.percent">
              <svg class="h-17.5 w-17.5 -rotate-90 transform">
                <circle
                  class="text-stroke dark:text-strokedark"
                  stroke-width="10"
                  stroke="currentColor"
                  fill="transparent"
                  r="30"
                  cx="35"
                  cy="35"
                ></circle>
                <circle
                  class="text-mining-green"
                  stroke-width="10"
                  :stroke-dasharray="30 * 2 * Math.PI"
                  :stroke-dashoffset="30 * 2 * Math.PI - (item.percent / 100) * 30 * 2 * Math.PI"
                  stroke="currentColor"
                  fill="transparent"
                  r="30"
                  cx="35"
                  cy="35"
                ></circle>
              </svg>
            </div>
          </div>
        </div>
      </template>
    </div>

</template>