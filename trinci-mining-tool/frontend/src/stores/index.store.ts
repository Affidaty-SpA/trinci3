import { defineStore } from "pinia";

export interface IRootStore {
  showLoader: boolean,
  isLoaderEnabled: boolean,
  loaderMessage: string[]
}

const initRootStore: IRootStore = {
  showLoader: false,
  isLoaderEnabled: false,
  loaderMessage: []
}

export const useRootStore = defineStore({
  id: "rootStore",
  state: () => ({
    ...initRootStore
  }),
  getters: {},
  actions: {
    setShowLoader(payload: boolean, message?: string) {
      this.showLoader = payload;
      this.isLoaderEnabled = payload;
      if(message) {
        this.loaderMessage.push(message)
      }
      if(!payload) {
        this.loaderMessage = []
      }
    },
  },
})
