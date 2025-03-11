<script setup lang="ts">
import { useSidebarStore } from '@/stores/sidebar'
import { onClickOutside } from '@vueuse/core'
import { onMounted, ref } from 'vue'
import SidebarItem from './SidebarItem.vue'
import { useNodeStore } from '@/stores/node'
import { storeToRefs } from 'pinia'
import type { ISocketNodeUpdate } from '@/models'
import type { IMenuGroup, IMenuItem } from '@/models/sidebar'

const target = ref(null)
const { setSidebarMenu } = useSidebarStore();
const { isSidebarOpen, menuRefs } = storeToRefs(useSidebarStore())
const { socketNodeData, activeNodes, lastBlockHeight, activeMiningNodes, otherNodes, allNodesData, currentAccountId } = storeToRefs(useNodeStore())
const { setGenericNodesData } = useNodeStore()

// della forma [menuItems disabled, menuItems active, menuItems disabled]
const menuGroupsRef = ref<IMenuGroup[]>([
  { menuItems: [] },
  { name: 'Your HBSD', menuItems: [] },
  { menuItems: [] }
])

onClickOutside(target, () => {
  isSidebarOpen.value = false
})

const mapInitOtherNodeData = (obj: any) => {
  return {
    node_indicator_1: false,
    node_indicator_2: false,
    node_indicator_3: false,
    node_id: obj.account_id,
    node_alignment: false,
    node_value_stake_a: obj.t_coin,
    node_value_stake_b: obj.t_coin,
    node_value_bitbel: 0,
    route: ``,
    node_classes: "player",
    node_disabled: true,
    node_selected: false,
  }
}

const last = <T>(a: T[]) => {
  return a[a.length - 1];
}

const mapInitOurNodeData = ([accountId, obj]: [string, any]) => {

  const life_points = last<any>(obj.stats.life_points)
  const tcoin = last<any>(obj.stats.tcoin)
  const bitbel = last<any>(obj.stats.bitbel)

  return {
    node_indicator_1: life_points.units >= 1 ? true : false,
    node_indicator_2: life_points.units >= 2 ? true : false,
    node_indicator_3: life_points.units == 3 ? true : false,
    node_id: accountId,
    node_alignment: lastBlockHeight.value == 0 ? true : obj.health.block_height < lastBlockHeight.value - 1 ? true : false, // TODO: il 10 dovrebbe essere la soglia per considerare il nodo in allineamento
    node_value_stake_a: tcoin.units_in_stake,
    node_value_stake_b: tcoin.total,
    node_value_bitbel: bitbel.units,
    route: `/dashboard/node/${accountId}`,
    node_classes: otherNodes.value.some(player => player.account_id === accountId) ? 'player' : 'npc',
    node_disabled: false, // TODO: Controllare se e` in allineamento o no
    node_selected: false
  }
}

/* const handleInitNodeData = () => {
  console.log(otherNodes.value)
  const separator = Math.round(otherNodes.value.length / 2)
  const itemsMenu = otherNodes.value.map(obj => {
    return {
      node_indicator_1: false,
      node_indicator_2: false,
      node_indicator_3: false,
      node_id: obj.account_id,
      node_alignment: true,
      node_value_stake_a: obj.t_coin,
      node_value_stake_b: obj.t_coin,
      node_value_bitbel: 0,
      route: `/node-detail/&{nodeAccId}`,
      node_classes: "player",
      node_disabled: true,
      node_selected: false,
    }
  })

  menuGroupsRef.value[0].menuItems = itemsMenu.splice(0, separator);
  menuGroupsRef.value[1].menuItems = []
  menuGroupsRef.value[2].menuItems = itemsMenu;
} */

const handleSocketNodeData = () => {
  const alignOffset = -1
  const totalNodes = 10
  // {
  //   node_indicator_1: obj.pop_update.life_points.units >= 1 ? true : false,
  //   node_indicator_2: obj.pop_update.life_points.units <= 2 ? true : false,
  //   node_indicator_3: obj.pop_update.life_points.units == 3 ? true : false,
  //   node_id: obj.account_id.chop(5, 5, '...'),
  //   node_alignment: obj.block_height < last_block_height - offset ? true : false,
  //   node_value_stake_a: obj.pop_update.tcoin.units_in_stake,
  //   node_value_stake_b: obj.pop_update.tcoin.total,
  //   node_value_bitbel: obj.pop_update.bitbel.units,
  //   route: `/node-detail/&{nodeAccId}`, -> get from map
  //   node_classes: obj.players_update.some((player) => obj.account_id === player.account_id) ? "player" : "npc"
  //   node_disabled: obj.players_update.some((player) => obj.account_id === player.account_id) && !activeNodes.value.some((player) => obj.account_id === player[0]) //check only ["accountId", true] ? true : false,
  //   node_selected: false,
  // }
  const nodesArrayData = [...socketNodeData.value.values()]

  // Vettore per contenere i nodi "disabilitati" da inserire prima e dopo gli active
  const otherNodes: Map<string, IMenuItem> = new Map()

  nodesArrayData.map((socketObj: ISocketNodeUpdate) => {
    // if is active
    const currentNode = {
      node_indicator_1: socketObj.pop_update.life_points.units >= 1 ? true : false,
      node_indicator_2: socketObj.pop_update.life_points.units >= 2 ? true : false,
      node_indicator_3: socketObj.pop_update.life_points.units == 3 ? true : false,
      node_id: socketObj.account_id,
      node_alignment: socketObj.health_update.block_height < lastBlockHeight.value - alignOffset ? true : false,
      node_value_stake_a: socketObj.pop_update.tcoin.units_in_stake,
      node_value_stake_b: socketObj.pop_update.tcoin.total,
      node_value_bitbel: socketObj.pop_update.bitbel.units,
      route: `/dashboard/node/${socketObj.account_id}`,
      node_classes: socketObj.players_update.some((player) => socketObj.account_id === player.account_id) ? 'player' : 'npc',
      node_disabled: socketObj.players_update.some((player) => socketObj.account_id === player.account_id) && !activeNodes.value.some((player) => socketObj.account_id === player[0]) ? true : false,
      node_selected: false
    }

    // MIA HDBS
    // Nodo player <=> e` tra quelli attivi && appare nel vettore players_update
    // Nodo NPC <=> e` tra quelli attivi && !appare nel vettore players_update
    // Others <=> !e` tra quelli attivi && appare nel vettore players_update


    const nodeInPlayerUpdate = socketObj.players_update.find((i) => i.account_id === socketObj.account_id)
    const isInPlayerUpdate = !!nodeInPlayerUpdate
    const isInActiveNodesList = !!activeNodes.value.find((i) => i[0] === socketObj.account_id)

    socketObj.players_update.map((player) => {
      if (!activeNodes.value.find(node => node[0] === player.account_id)) {
        otherNodes.set(player.account_id, {
          node_indicator_1: player.life_points >= 1 ? true : false,
          node_indicator_2: player.life_points >= 2 ? true : false,
          node_indicator_3: player.life_points == 3 ? true : false,
          node_id: player.account_id,
          node_alignment: false,
          node_value_stake_a: player.units_in_stake,
          node_value_stake_b: 0,
          node_value_bitbel: 0,
          route: ``, // -> get from map
          node_classes: 'player',
          node_disabled: true,
          node_selected: false
        })
      }
    })


    // e` nel player update e in activeNodes
    if (isInPlayerUpdate && isInActiveNodesList) {
      menuGroupsRef.value[1].menuItems.push(currentNode)
      // e` nei player update ma non in active nodes
    } else if (!isInPlayerUpdate && isInActiveNodesList) {
      menuGroupsRef.value[1].menuItems.push(currentNode)
    }

    // activeNodeCount: number;
    // disabledNodeCount: number;
    // playerCount: number;
    // npcCount: number;
    // totalPlayers: number;
    // totalBitbel: number;
    // tCoinInStake: number;
    // totalTCoin: number;

    const genericNodesData = {
      miningStatus: activeNodes.value.some(i => i[1]),
      activeNodeCount: activeMiningNodes.value.filter((i) => i[1]).length,
      disabledNodeCount: activeMiningNodes.value.filter((i) => !i[1]).length,
      playerCount: menuGroupsRef.value[1].menuItems.filter((i) => i.node_classes === 'player').length,
      npcCount: menuGroupsRef.value[1].menuItems.filter((i) => i.node_classes === 'npc').length,
      totalPlayers: socketObj.players_update.length,
      totalBitbel: menuGroupsRef.value[1].menuItems.reduce((acc, curr) => {
        acc += curr.node_value_bitbel;
        return acc;
      }, 0),
      tCoinInStake: menuGroupsRef.value[1].menuItems.reduce((acc, curr) => {
        acc += curr.node_value_stake_a;
        return acc;
      }, 0),
      totalTCoin: menuGroupsRef.value[1].menuItems.reduce((acc, curr) => {
        acc += curr.node_value_stake_b
        return acc;
      }, 0)
    }
    setGenericNodesData(genericNodesData);
  })


  const otherNodesArray = [...otherNodes.values()]
  const separator = Math.round(otherNodesArray.length / 2)

  menuGroupsRef.value[0].menuItems = otherNodesArray.splice(0, separator)
  menuGroupsRef.value[2].menuItems = otherNodesArray

  //Sort npc and player
  menuGroupsRef.value[1].menuItems.sort((a, b) => a.node_classes > b.node_classes ? -1 : 1)

  setSidebarMenu(menuGroupsRef.value)

}

/* const menuGroups = ref([
  // Disabled Nodes
  {
    menuItems: [
      {
        node_indicator_1: true,
        node_indicator_2: true,
        node_indicator_3: false,
        node_id: 'USSIO..JS678',
        node_alignment: false,
        node_value_stake_a: '25',
        node_value_stake_b: '3K',
        node_value_bitbel: '21',
        route: '/dashboard/home',
        node_classes: 'player', // type "player" or "npc"
        node_disabled: true,
        node_selected: false
      },
      {
        node_indicator_1: true,
        node_indicator_2: true,
        node_indicator_3: false,
        node_id: 'USSIO..JS678',
        node_alignment: false,
        node_value_stake_a: '25',
        node_value_stake_b: '3K',
        node_value_bitbel: '21',
        route: '/dashboard/home',
        node_classes: 'player', // type "player" or "npc"
        node_disabled: true,
        node_selected: false
      },
      {
        node_indicator_1: true,
        node_indicator_2: true,
        node_indicator_3: false,
        node_id: 'USSIO..JS678',
        node_alignment: false,
        node_value_stake_a: '25',
        node_value_stake_b: '3K',
        node_value_bitbel: '21',
        route: '/dashboard/home',
        node_classes: 'player', // type "player" or "npc"
        node_disabled: true,
        node_selected: false
      }
    ]
  },
  // Active Nodes
  {
    name: 'Your HDSB',
    menuItems: [
      {
        node_indicator_1: true,
        node_indicator_2: false,
        node_indicator_3: false,
        node_id: 'USSIO..JS678',
        node_alignment: false,
        node_value_stake_a: '25',
        node_value_stake_b: '3K',
        node_value_bitbel: '21',
        route: '/dashboard/home',
        node_classes: 'player', // type "player" or "npc"
        node_disabled: false,
        node_selected: false
      },
      {
        node_indicator_1: true,
        node_indicator_2: true,
        node_indicator_3: false,
        node_id: 'USSIO..JS678',
        node_alignment: false,
        node_value_stake_a: '25',
        node_value_stake_b: '3K',
        node_value_bitbel: '21',
        route: '/dashboard/home',
        node_classes: 'player', // type "player" or "npc"
        node_disabled: false,
        node_selected: true
      },
      {
        node_indicator_1: true,
        node_indicator_2: true,
        node_indicator_3: false,
        node_id: 'USSIO..JS678',
        node_alignment: true,
        node_value_stake_a: '25',
        node_value_stake_b: '3K',
        node_value_bitbel: '21',
        route: '/dashboard/home',
        node_classes: 'player', // type "player" or "npc"
        node_disabled: false,
        node_selected: false
      },
      {
        node_indicator_1: true,
        node_indicator_2: true,
        node_indicator_3: false,
        node_id: 'USSIO..JS678',
        node_alignment: false,
        node_value_stake_a: '25',
        node_value_stake_b: '3K',
        node_value_bitbel: '21',
        route: '/dashboard/home',
        node_classes: 'npc', // type "player" or "npc"
        node_disabled: false,
        node_selected: false
      }
    ]
  },
  // Disabled Nodes
  {
    menuItems: [
      {
        node_indicator_1: true,
        node_indicator_2: true,
        node_indicator_3: false,
        node_id: 'USSIO..JS678',
        node_alignment: false,
        node_value_stake_a: '25',
        node_value_stake_b: '3K',
        node_value_bitbel: '21',
        route: '/dashboard/home',
        node_classes: 'player', // type "player" or "npc"
        node_disabled: true,
        node_selected: false
      },
      {
        node_indicator_1: true,
        node_indicator_2: true,
        node_indicator_3: false,
        node_id: 'USSIO..JS678',
        node_alignment: false,
        node_value_stake_a: '25',
        node_value_stake_b: '3K',
        node_value_bitbel: '21',
        route: '/dashboard/home',
        node_classes: 'player', // type "player" or "npc"
        node_disabled: true,
        node_selected: false
      },
      {
        node_indicator_1: true,
        node_indicator_2: true,
        node_indicator_3: false,
        node_id: 'USSIO..JS678',
        node_alignment: false,
        node_value_stake_a: '25',
        node_value_stake_b: '3K',
        node_value_bitbel: '21',
        route: '/dashboard/home',
        node_classes: 'player', // type "player" or "npc"
        node_disabled: true,
        node_selected: false
      }
    ]
  }
]) */

const handleSidebarItemSelection = (accountId: string) => {
  menuGroupsRef.value[1].menuItems = menuGroupsRef.value[1].menuItems.map((menuItem) => {
    menuItem.node_selected = (menuItem.node_id === accountId)? true: false
    return menuItem
  })
}

onMounted(() => {
  if(menuRefs.value.length) {
    menuGroupsRef.value = menuRefs.value
  } else {
    // handleInitNodeData()
    // handleSocketNodeData()
  }
})
</script>
<template>
  <aside
    class="absolute left-0 top-0 z-9999 flex h-screen w-80 lg:w-100 flex-col overflow-y-hidden bg-black duration-300 ease-linear dark:bg-boxdark lg:static lg:translate-x-0"
    :class="{
      'translate-x-0': isSidebarOpen,
      '-translate-x-full': !isSidebarOpen
    }" ref="target">
    <!-- SIDEBAR HEADER -->
    <div class="flex items-center justify-between gap-2 px-6 py-5.5 lg:pt-6.5 lg:pb-5.5">
      <router-link to="/">
        <img src="@/assets/images/logo/logo.svg" alt="Logo" />
      </router-link>

      <button class="block lg:hidden" @click="isSidebarOpen = false">
        <svg class="fill-current" width="20" height="18" viewBox="0 0 20 18" fill="none"
          xmlns="http://www.w3.org/2000/svg">
          <path
            d="M19 8.175H2.98748L9.36248 1.6875C9.69998 1.35 9.69998 0.825 9.36248 0.4875C9.02498 0.15 8.49998 0.15 8.16248 0.4875L0.399976 8.3625C0.0624756 8.7 0.0624756 9.225 0.399976 9.5625L8.16248 17.4375C8.31248 17.5875 8.53748 17.7 8.76248 17.7C8.98748 17.7 9.17498 17.625 9.36248 17.475C9.69998 17.1375 9.69998 16.6125 9.36248 16.275L3.02498 9.8625H19C19.45 9.8625 19.825 9.4875 19.825 9.0375C19.825 8.55 19.45 8.175 19 8.175Z"
            fill="" />
        </svg>
      </button>
    </div>
    <!-- SIDEBAR HEADER -->

    <!-- TODO: calc nodes number and active specific layout  -->
    <!-- Sidebar wrapper no scroll -->
    <!-- <div class="sidebar--noscroll no-scrollbar"
      style="background-image: url('/src/assets/images/illustration/bg-sidebar.svg')">
      <div class="sidebar--noscroll__gradient--top"></div>
      <div class="sidebar--noscroll__gradient--bottom"></div>
      <nav class="sidebar--noscroll__menu--centered lg:mt8 lg:px-6">
        <template v-for="menuGroup in menuGroupsRef" :key="menuGroup.name">
          <div class="sidebar--noscroll__menu-group">
            <h3 class="sidebar__menu-group__title">{{ menuGroup.name }}</h3>
            <div class="sidebar__menu-group__items">
              <SidebarItem v-for="(menuItem, index) in menuGroup.menuItems" :item="menuItem" :key="index"
                :index="index" />
            </div>
          </div>
        </template>
</nav>
</div> -->
    <!-- Sidebar wrapper without scroll -->

    <!-- Sidebar wrapper with scroll -->
    <!-- Added this styles for activate fix bg in sidebar:  style="background-image: url('/src/assets/images/illustration/bg-sidebar.svg')" -->
    <div class="sidebar--withscroll no-scrollbar">
      <!-- Sidebar Menu -->
      <nav class="sidebar--withscroll__menu">
        <!-- PRE OTHER NODES -->
        <template v-for="(menuGroup, index) in otherNodes.filter(p => p.account_id !== currentAccountId).map(mapInitOtherNodeData)" :key="index" >
          <div v-if="index < Math.round(otherNodes.length) / 2">
            <h3 class="sidebar__menu-group__title"></h3>
            <ul class="sidebar__menu-group__items">
              <SidebarItem :item="menuGroup" :key="menuGroup.node_id" :index/>
            </ul>
          </div>
        </template>
        <!-- OUR PLAYERS AND NPC -->
        <template v-for="(menuGroup, index) in [...allNodesData].map(mapInitOurNodeData)" :key="menuGroup.index">
          <div>
            <h3 class="sidebar__menu-group__title">My HDSB</h3>
            <ul class="sidebar__menu-group__items">
              <SidebarItem :item="menuGroup" @itemSelected="handleSidebarItemSelection" :key="menuGroup.node_id" :index/>
            </ul>
          </div>
        </template>
        <!-- POST OTHER NODES -->
        <template v-for="(menuGroup, index) in otherNodes.filter(p => p.account_id !== currentAccountId).map(mapInitOtherNodeData)" :key="menuGroup.index">
          <div v-if="index >= Math.round(otherNodes.length) / 2">
            <h3 class="sidebar__menu-group__title"></h3>
            <ul class="sidebar__menu-group__items">
              <SidebarItem :item="menuGroup" :key="menuGroup.node_id" :index />
            </ul>
          </div>
        </template>
      </nav>
      <!-- Sidebar Menu -->
    </div>
    <!-- Sidebar wrapper with scroll -->
  </aside>
</template>
<style scoped>
@import 'tailwindcss/components';

/* Added class "hidden" at the ".sidebar--noscroll" for remove */
.sidebar--noscroll {
  @apply flex flex-col duration-300 ease-linear h-screen relative overflow-hidden bg-right bg-repeat-y;
}

.sidebar--noscroll__gradient--top {
  @apply w-full h-[200px] bg-gradient-to-b from-boxdark to-transparent absolute -top-[1px] right-1 z-1;
}

.sidebar--noscroll__gradient--bottom {
  @apply w-full h-[200px] bg-gradient-to-t from-boxdark to-transparent absolute bottom-0 right-1 z-1;
}

.sidebar--noscroll__menu--centered {
  @apply mt-0 py-0 px-4 absolute top-1/2 left-1/2 transform -translate-y-1/2 -translate-x-1/2 w-full;
}

.sidebar--noscroll__menu-group {
  @apply w-full;
}

.sidebar__menu-group__title {
  @apply mb-4 ml-0 text-base font-bold text-gray;
}

.sidebar__menu-group__items {
  @apply mb-6 flex flex-col gap-4;
}

/* Added class "hidden" at the ".sidebar--withscroll" for remove */
.sidebar--withscroll {
  @apply flex flex-col overflow-y-auto duration-300 ease-linear relative bg-right bg-repeat-y;
}

.sidebar--withscroll__menu {
  @apply mt-0 pb-4 px-4;
}
</style>