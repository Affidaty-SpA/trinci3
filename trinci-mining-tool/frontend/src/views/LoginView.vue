<script setup lang="ts">
import { useAuthIn } from '@/utils/useAuthin';

import { Browser } from '@capacitor/browser'
import { initAccount } from '@/utils/utils'
import { useUserStore } from '@/stores/user.store'
import { useRouter } from 'vue-router'
import type { TrinciTokenAuth } from '@/router/index';

const { login } = useAuthIn()
const { setUserAccount, setUserInformations } = useUserStore();
const router = useRouter()

const openInApp = async (url: string) => {
  await Browser.open({ url: url })
}

// Set trinciToken ->
const loginTrigger = async () => {
  return login().then(async(res) => {
    const tokenToStore: TrinciTokenAuth = { token : res.token , expire : new Date(res.tokenExpire)};
    localStorage.setItem("trinci_token", JSON.stringify({token: tokenToStore.token, expire: new Date(res.tokenExpire).getTime()}));
    return initAccount(res.userInfo.user.publicKey).then((account) => {
      setUserAccount(account); // This fn also set account in localStorage
      setUserInformations(res.userInfo.user); // This fn also set userInformations in localStorage
      router.replace({ name: 'checkStatus' })
      return res;
    })
  })
}

</script>
<template>

  <section class="overflow-hidden px-4 sm:px-8" style="">
    <div class="flex h-screen flex-col items-center justify-center overflow-hidden">
      <div class="no-scrollbar overflow-y-auto py-20">
        <div class="mx-auto w-full max-w-[480px]">
          <div class="text-center">
            <router-link to="/" class="mx-auto mb-10 inline-flex">
              <img src="@/assets/images/logo/logo-dark.svg" alt="logo" class="dark:hidden" />
              <img src="@/assets/images/logo/logo.svg" alt="logo" class="hidden dark:block" />
            </router-link>

            <div class=" lg:p-7.5 xl:p-12.5">
               <div class="flex justify-center pb-20">
                <img src="@/assets/images/illustration/goldmining.svg">
               </div>

                <button  @click="loginTrigger()"
                  class="flex w-full justify-center rounded-md bg-primary p-[13px] font-bold text-black hover:bg-opacity-90"
                >
                Auth-in with ARYA
                </button>

                <p class="mt-10">v.1.0</p>
                <button class="mt-5 block text-white text-xs hidden" @click="
                  openInApp('https://legal.affidaty.io')
                ">
                  Termini e condizioni (edit link to right terms & condition)
              </button>

            </div>
          </div>
        </div>
      </div>
    </div>
  </section>

</template>
<style scoped>
@import 'tailwindcss/components';
section {
  @apply bg-center bg-cover;
  background-image: url(@/assets/images/illustration/bg-login.svg);
}
</style>
