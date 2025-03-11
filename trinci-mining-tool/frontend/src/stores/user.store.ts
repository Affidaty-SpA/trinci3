import type { TrinciTokenAuth } from "@/router";
import { initAccount } from "@/utils/utils";
import WSClient from "@/utils/wsClient";
import { defineStore } from "pinia";
import { type NavigationGuardNext } from "vue-router";
import { useToast } from "vue-toastification";
import type { Account } from "@affidaty/t2-lib-core";
import { useRootStore } from "./index.store";
import { type UserData } from "@/utils/useAuthin";

const toast = useToast();

export interface UserStore {
  userInformations: UserData;
  account: Account;
  userRole: string;
  permissions: any;
}

const initUserStore: UserStore = {
  userInformations: {} as UserData,
  account: {} as Account,
  userRole: "UNKNOW",
  permissions: {}
}

export const useUserStore = defineStore({
  id: "userStore",
  state: () => ({
    ...initUserStore
  }),
  getters: {},
  actions: {
    setUserAccount(payload: Account) {
      localStorage.setItem("user_account", JSON.stringify(payload))
      this.account = payload;
    },
    setUserInformations(payload: UserData) {
      localStorage.setItem("user_information", JSON.stringify(payload))
      this.userInformations = payload;
    },
    beforeEnterPage(token: TrinciTokenAuth, next: NavigationGuardNext) {
      if (localStorage.getItem("user_account") && localStorage.getItem("user_information")) {
        const userInformation: UserData = JSON.parse(localStorage.getItem("user_information") as string);
        initAccount(userInformation.publicKey).then((account: Account) => {
          this.account = account
          this.setUserAccount(account);
          this.setUserInformations(userInformation);

          WSClient.getSocket(token.token).then((client) => {
            // console.log('Socket is connected? ', client.socket.connected);
            const { setShowLoader } = useRootStore()
            setShowLoader(true)

            client.joinRoom(account.accountId, account.accountId)
            this.userRole = "UNKNOW"
            // console.log("User auth OK!");

            /*
            client.asyncRequest<{[key:string]:string}, { accountId: string }>('SocketService....', { accountId: account.accountId })
              .then((res) => {

              // If user can stay in this app
              // TODO: set user role
              // Fetch here your mandatory data & set stores before enter page
              // else
              // return useAuthIn().logout(() => {
              // this.clearStorageAndLogout("Your not authorized!!!!!!!!")
              //  return next({ name: 'login' })
              //}
            }).catch((e) => {
              return useAuthIn().logout(() => {
                this.clearStorageAndLogout("Your not authorized!!!!!!!!");
                return next({ name: 'login' })
              })
            })*/
            setShowLoader(false)
            next();
          }).catch(console.error)
        })
      } else {
        // information missing in local storage
        console.log('information missing in local storage')
        return next({ name: 'login' })
      }
    },
    clearStorageAndLogout(message?:string) {
      if (message) toast.error(message)
      localStorage.removeItem('user_account')
      localStorage.removeItem('userInformations')
      localStorage.removeItem('account')
      localStorage.removeItem('user_information')
      localStorage.removeItem('trinci_token')
      this.resetUserStore();
    },
    resetUserStore() {
      this.userInformations = {} as UserData
      this.account = {} as Account
      this.userRole = "UNKNOW"
      this.permissions = {}
    }
  }
})
