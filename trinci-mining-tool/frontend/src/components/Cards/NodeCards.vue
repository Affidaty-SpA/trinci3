<script setup lang="ts">
import { computed, onMounted, ref, watchPostEffect } from 'vue';
import { InformationCircleIcon } from '@heroicons/vue/24/outline';
import { storeToRefs } from 'pinia';
import { useNodeStore } from '@/stores/node';

const props = defineProps<{
  accountId: string;
}>();

const { isAlive, genericNodesData, nodeData } = storeToRefs(useNodeStore());
const nodeAlive = computed<boolean>(() => isAlive.value(nodeData?.value?.health.last_time || 0));
const dataList = ref<Record<string, any>[]>([])

// {
//     "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4qhP",
//     "health_update": {
//         "block_height": 615,
//         "last_time": 1711637156,
//         "system_stats": {
//             "cpus_usage": {
//                 "measure": "Percent",
//                 "total": 100,
//                 "used": 94
//             },
//             "disk_usage": {
//                 "measure": "Bytes",
//                 "total": 2046659112960,
//                 "used": 1632914755584
//             },
//             "mem_usage": {
//                 "measure": "Bytes",
//                 "total": 16315424768,
//                 "used": 8429645824
//             }
//         },
//         "unconfirmed_pool_len": 0
//     },
//     "players_update": [
//         {
//             "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4qhP",
//             "life_points": 2,
//             "units_in_stake": 11300
//         },
//         {
//             "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4XXX",
//             "life_points": 3,
//             "units_in_stake": 100
//         },
//         {
//             "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4YYY",
//             "life_points": 2,
//             "units_in_stake": 120
//         },
//         {
//             "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4KKK",
//             "life_points": 1,
//             "units_in_stake": 10000
//         },
//         {
//             "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4JJJ",
//             "life_points": 3,
//             "units_in_stake": 10303
//         },
//         {
//             "account_id": "QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cvNL",
//             "life_points": 3,
//             "units_in_stake": 27101
//         }
//     ],
//     "pop_update": {
//         "bitbel": {
//             "time": 1711637142,
//             "units": 695000
//         },
//         "life_points": {
//             "time": 1711637142,
//             "total": 3,
//             "units": 2
//         },
//         "tcoin": {
//             "time": 1711637142,
//             "total": 27101,
//             "units_in_stake": 11300
//         }
//     }
// }

const last = <T>(a: T[]) => {
  return a[a.length - 1];
}

const refreshData = () => {
  if (Object.keys(nodeData?.value || {}).length > 0) {
    const lifepoints = last<any>(nodeData?.value?.stats.life_points as []);
    const btb =  last<any>(nodeData!.value?.stats.bitbel as []);
    const tCoin = last<any>(nodeData!.value?.stats.tcoin as []);
    dataList.value = [
      {
        title: 'Life points',
        total:lifepoints.units || 0,
        totalgeneral:lifepoints.total || 0,
        percent: (lifepoints.total != 0) ? parseInt((100 * lifepoints.units / lifepoints.total!).toFixed(0)) : 0,
        live: nodeAlive.value
      },
      {
        title: 'Bitbel',
        total: btb.units || 0,
        button1: {
          title: 'Send',
          disabled: true,
          handler: () => {console.log('budubu 1')}
        },
        button2: {
          title: 'Withdraw',
          handler: () => {console.log('budubu 2')}
        },
      },
      {
        title: `#Tcoin in stake`,
        total:  tCoin.units_in_stake,
        totalgeneral: tCoin.total,
        percent: parseInt((((tCoin.units_in_stake || 0) / tCoin.total) * 100).toFixed(0)),
        button1: {
          title: "Handle Stake",
          disabled: tCoin.units_in_stake === 0,
          handler: () => {console.log('budubu')} //TODO: handle modal stake
        }
      },
      {
        title: 'Unconfirmed pool',
        total: nodeData!.value?.health?.unconfirmed_pool_len || 0,
      }
    ]
  }
}

watchPostEffect(() => {
  if (nodeData?.value) {
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
                <div v-if="item.live" class="inline-flex space-x-1.5 node-indicator ml-5">
                  <div v-for="life in item.totalgeneral" class="h-3 w-3 rounded-full bg-mining-green" :class="{'opacity-30' : life > item.total}"></div>
                </div>
              </h3>
              <p class="font-medium">{{ item.title }}</p>
              <div class="mt-2 flex items-center gap-2">

                <button v-if="item.button1" :disabled="item.button1?.disabled" @click="() => item.button1.handler()"
                  class="flex items-center gap-1 rounded-md p-1 text-md font-bold text-form-input bg-mining-blue hover:bg-mining-green px-5"
                > <span>{{ item.button1.title }}</span>
                </button>
                <button v-if="item.button2" :disabled="item.button2?.disabled" @click="() => item.button2.handler()"
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