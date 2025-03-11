<script setup lang="ts">
import { useUserStore } from '@/stores/user.store';
import { useAuthIn } from '@/utils/useAuthin';
import WSClient from '@/utils/wsClient';
import { onBeforeMount, onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import { useToast } from 'vue-toastification';
import { EphemeralTx } from '@/utils/trinci';
import DashboardLayout from '@/layouts/DashboardLayout.vue';
import ChartNetwork from '@/components/Charts/ChartNetwork.vue';
import ChartMining from '@/components/Charts/ChartMining.vue';
import DashboardCards from '@/components/Cards/DashboardCards.vue'
import { useNodeStore } from "@/stores/node";

const { clearStorageAndLogout } = useUserStore();
const { logout } = useAuthIn();
const router = useRouter();
const { updateSocketNodeData, setActiveNodes, setActiveMiningNodes, initMiningTool } = useNodeStore();
const toast = useToast();
const nodeAddresses = ref<{ value: string; enabled: boolean }[]>([]);

const params = ref<Record<string, any>>({
  target: '',
  method: '',
  args: '{"a": 1}',
  url: '/mining/rpc'
})


const logoutTrigger = () => {
  logout();
  clearStorageAndLogout();
  router.replace({ name: 'login' });
  WSClient.disconnect();
}

const testSign = () => {
  return new EphemeralTx('#account', 'set_identity', { args: "" })
}

const handleFirmaGiue = () => {
  return new EphemeralTx(params.value.target, params.value.method, JSON.parse(params.value.args))

}

onMounted(() => {
  initMiningTool();
})

onBeforeMount(() => {
  // updateSocketNodeData({
  //   "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4qhP",
  //   "health_update": {
  //     "block_height": 615,
  //     "last_time": 1711637156,
  //     "system_stats": {
  //       "cpus_usage": {
  //         "measure": "Percent",
  //         "total": 100,
  //         "used": 94
  //       },
  //       "disk_usage": {
  //         "measure": "Bytes",
  //         "total": 2046659112960,
  //         "used": 1632914755584
  //       },
  //       "mem_usage": {
  //         "measure": "Bytes",
  //         "total": 16315424768,
  //         "used": 8429645824
  //       }
  //     },
  //     "unconfirmed_pool_len": 0
  //   },
  //   "players_update": [
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4qhP",
  //       "life_points": 2,
  //       "units_in_stake": 11300
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4XXX",
  //       "life_points": 3,
  //       "units_in_stake": 100
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4YYY",
  //       "life_points": 2,
  //       "units_in_stake": 120
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4KKK",
  //       "life_points": 1,
  //       "units_in_stake": 10000
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4JJJ",
  //       "life_points": 3,
  //       "units_in_stake": 10303
  //     },
  //     {
  //       "account_id": "QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cvNL",
  //       "life_points": 3,
  //       "units_in_stake": 27101
  //     }
  //   ],
  //   "pop_update": {
  //     "bitbel": {
  //       "time": 1711637142,
  //       "units": 695000
  //     },
  //     "life_points": {
  //       "time": 1711637142,
  //       "total": 3,
  //       "units": 2
  //     },
  //     "tcoin": {
  //       "time": 1711637142,
  //       "total": 27101,
  //       "units_in_stake": 11300
  //     }
  //   }
  // })
  // updateSocketNodeData({
  //   "account_id": "QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2czIO",
  //   "health_update": {
  //     "block_height": 615,
  //     "last_time": 1711637156,
  //     "system_stats": {
  //       "cpus_usage": {
  //         "measure": "Percent",
  //         "total": 100,
  //         "used": 94
  //       },
  //       "disk_usage": {
  //         "measure": "Bytes",
  //         "total": 2046659112960,
  //         "used": 1632914755584
  //       },
  //       "mem_usage": {
  //         "measure": "Bytes",
  //         "total": 16315424768,
  //         "used": 8429645824
  //       }
  //     },
  //     "unconfirmed_pool_len": 90
  //   },
  //   "players_update": [
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4qhP",
  //       "life_points": 2,
  //       "units_in_stake": 11300
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4XXX",
  //       "life_points": 3,
  //       "units_in_stake": 100
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4YYY",
  //       "life_points": 2,
  //       "units_in_stake": 120
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4KKK",
  //       "life_points": 1,
  //       "units_in_stake": 10000
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4JJJ",
  //       "life_points": 3,
  //       "units_in_stake": 10303
  //     },
  //     {
  //       "account_id": "QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cvNL",
  //       "life_points": 3,
  //       "units_in_stake": 27101
  //     }
  //   ],
  //   "pop_update": {
  //     "bitbel": {
  //       "time": 1711637142,
  //       "units": 1900
  //     },
  //     "life_points": {
  //       "time": 1711637142,
  //       "total": 3,
  //       "units": 0
  //     },
  //     "tcoin": {
  //       "time": 1711637142,
  //       "total": 777,
  //       "units_in_stake": 0
  //     }
  //   }
  // })
  // updateSocketNodeData({
  //   "account_id": "QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cvNL",
  //   "health_update": {
  //     "block_height": 615,
  //     "last_time": 1711637156,
  //     "system_stats": {
  //       "cpus_usage": {
  //         "measure": "Percent",
  //         "total": 100,
  //         "used": 94
  //       },
  //       "disk_usage": {
  //         "measure": "Bytes",
  //         "total": 2046659112960,
  //         "used": 1632914755584
  //       },
  //       "mem_usage": {
  //         "measure": "Bytes",
  //         "total": 16315424768,
  //         "used": 8429645824
  //       }
  //     },
  //     "unconfirmed_pool_len": 0
  //   },
  //   "players_update": [
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4qhP",
  //       "life_points": 2,
  //       "units_in_stake": 11300
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4XXX",
  //       "life_points": 3,
  //       "units_in_stake": 100
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4YYY",
  //       "life_points": 2,
  //       "units_in_stake": 120
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4KKK",
  //       "life_points": 1,
  //       "units_in_stake": 10000
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4JJJ",
  //       "life_points": 3,
  //       "units_in_stake": 10303
  //     },
  //     {
  //       "account_id": "QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cvNL",
  //       "life_points": 3,
  //       "units_in_stake": 27101
  //     }
  //   ],
  //   "pop_update": {
  //     "bitbel": {
  //       "time": 1711637142,
  //       "units": 695000
  //     },
  //     "life_points": {
  //       "time": 1711637142,
  //       "total": 3,
  //       "units": 3
  //     },
  //     "tcoin": {
  //       "time": 1711637142,
  //       "total": 27101,
  //       "units_in_stake": 27101
  //     }
  //   }
  // })
  // updateSocketNodeData({
  //   "account_id": "QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cABC",
  //   "health_update": {
  //     "block_height": 615,
  //     "last_time": 1711637156,
  //     "system_stats": {
  //       "cpus_usage": {
  //         "measure": "Percent",
  //         "total": 100,
  //         "used": 94
  //       },
  //       "disk_usage": {
  //         "measure": "Bytes",
  //         "total": 2046659112960,
  //         "used": 1632914755584
  //       },
  //       "mem_usage": {
  //         "measure": "Bytes",
  //         "total": 16315424768,
  //         "used": 8429645824
  //       }
  //     },
  //     "unconfirmed_pool_len": 0
  //   },
  //   "players_update": [
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4qhP",
  //       "life_points": 2,
  //       "units_in_stake": 11300
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4XXX",
  //       "life_points": 3,
  //       "units_in_stake": 100
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4YYY",
  //       "life_points": 2,
  //       "units_in_stake": 120
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4KKK",
  //       "life_points": 1,
  //       "units_in_stake": 10000
  //     },
  //     {
  //       "account_id": "QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4JJJ",
  //       "life_points": 3,
  //       "units_in_stake": 10303
  //     },
  //     {
  //       "account_id": "QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cvNL",
  //       "life_points": 3,
  //       "units_in_stake": 27101
  //     }
  //   ],
  //   "pop_update": {
  //     "bitbel": {
  //       "time": 1711637142,
  //       "units": 695000
  //     },
  //     "life_points": {
  //       "time": 1711637142,
  //       "total": 3,
  //       "units": 0
  //     },
  //     "tcoin": {
  //       "time": 1711637142,
  //       "total": 27101,
  //       "units_in_stake": 0
  //     }
  //   }
  // })
  // setActiveNodes([
  //   ["QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4qhP", true],
  //   ["QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cvNL", true],
  //   ["QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cvNZ", false],
  //   ["QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cvNY", false],
  //   ["QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cABC", true],
  //   ["QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4QQQ", true],
  //   ["QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2czIO", true],
  // ]);
  // setActiveMiningNodes([
  //   ["QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4qhP", true], // PLAYER
  //   ["QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cvNL", true], // PLAYER
  //   ["QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cABC", true], // NODE NPC
  //   ["QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4QQQ", true], // Not yet received update from this node
  //   ["QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2czIO", true],
  // ]);
})
</script>

<template>
  <DashboardLayout>
    <h1 class="page-title">Dashboard</h1>
    <!--4 boxes-->
    <DashboardCards />

    <!--end 4 boxes-->

    <!--Charts area-->
    <div class="mt-7.5 grid grid-cols-12 gap-4 md:gap-6 2xl:gap-7.5">
      <div class="col-span-12 xl:col-span-7">
        <ChartNetwork />
      </div>
      <div class="col-span-12 xl:col-span-5">
        <ChartMining />
      </div>
    </div>
    <!--end Charts area-->

    <!-- TEST AREA -->
    <!-- <h1 class="page-title mt-30 border-t-mining-green border-t-2 pt-5">TEST AREA</h1>
    <h2>For checkbox style:</h2>
    <input type="checkbox" name="a"> sdsd

    <input type="checkbox" name="a"> sds

    <div class="mt-4 grid grid-cols-12 gap-4 md:mt-6 md:gap-6 2xl:mt-7.5 2xl:gap-7.5">
      <h1 class="m-5 text-green-500">{{ nodeAddresses }}</h1>
      <button @click="logoutTrigger()" class="ring ring-red-500 rounded-full p-3 w-full mb-10">
        Logout
      </button>
      <button @click="testSign()" class="ring ring-red-500 rounded-full p-3 w-full mb-10">
        Test sign
      </button>
      <button @click="() => router.push('/wizard/step1')">
        Go to wizard
      </button>
      <button @click="() => router.go(-1)">Click</button>
    </div>

    <div>
      <form class="text-black">
        <span class=text-white>Target:</span>
        <input type="text" id="target" v-model="params.target"><br/><br/>
        <span class=text-white>Method:</span>
        <input type="text" id="method" v-model="params.method"><br/><br/>
        <span class=text-white>Arguments:</span>
        <textarea name="args" id="args" cols="30" rows="10" v-model="params.args"></textarea><br/><br/>
      </form>
      <button class="ring ring-red-500 rounded-full p-3 w-full mb-10 text-white" @click="handleFirmaGiue">Firma gi&ugrave;!</button>
    </div> -->
  </DashboardLayout>
</template>
