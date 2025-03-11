import { defineStore } from 'pinia'
import type { IMenuGroup, IMenuItem } from '@/models/sidebar'

export interface ISidebarStore {
  isSidebarOpen: boolean;
  selected: string;
  page: string;
  menuRefs: IMenuGroup[]
}

const initSidebarStore: ISidebarStore = {
  isSidebarOpen: false,
  selected: "eCommerce",
  page: "Dashboard",
  menuRefs: []
}

export const useSidebarStore = defineStore({
  id: 'sidebarStore',
  state: () => ({
    ...initSidebarStore
  }),
  getters: {},
  actions:{
    toggleSidebar(){
      this.isSidebarOpen = !this.isSidebarOpen
    },
    setSidebarMenu(payload: IMenuGroup[]) {
      this.menuRefs = payload
    }
  }
})
