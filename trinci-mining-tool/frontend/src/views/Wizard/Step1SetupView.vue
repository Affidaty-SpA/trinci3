<script setup lang="ts">
import WizardLayout from '@/layouts/WizardLayout.vue'
import { useRouter } from 'vue-router'
import { useNodeStore } from '@/stores/node'
import { useRootStore } from '@/stores/index.store'
import { storeToRefs } from 'pinia'

const { scanNetwork, setErrorMsg } = useNodeStore()
const { setShowLoader } = useRootStore()
const { errorMsg } = storeToRefs(useNodeStore())
const router = useRouter()

const newScan = () => {
    setShowLoader(true);
    scanNetwork(() => router.push('/wizard/step2')).then(() => {
        setTimeout(() => {
            setShowLoader(false)
        }, 2000)
    }).catch((e) => {
        setErrorMsg(e)
    })
}

// const connectToWebSocket = () => {
//     const ws = new WebSocket("ws://127.0.0.1:7000/ws")
//     console.log(ws)

//     ws.addEventListener('open', (event) => {
//         console.log(event)
//     })

//     ws.onmessage = (event) => {
//     console.log(event.data);
//     const jsonData = JSON.parse(event.data);
//     console.log(`ws: ${jsonData}`);
//     // updatePlayersInfo(jsonData);
//     };

//     ws.onerror = (error) => {
//     console.error("WebSocket error:", error);
//     };

//     ws.onclose = () => {
//     console.log("WebSocket connection closed");
//     };
// };

</script>

<template>
    <WizardLayout>
        <div class="h-full relative">
            <p class="text-sm sm:hidden font-medium text-gray-900 mb-6">Chain Setup...</p>
            <div class="" aria-hidden="true">
                <div class="overflow-hidden rounded-full bg-black">
                    <div class="h-2 rounded-full bg-primary" style="width: 25%" />
                </div>
                <div class="mt-6 hidden grid-cols-3 text-sm font-medium text-gray-600 sm:grid">
                    <div class="text-primary">Chain Setup</div>
                    <div class="text-center">Nodes Found</div>
                    <div class="text-right">Chain Updated!</div>
                </div>
            </div>
            <div class="mt-12 lg:mt-20 text-center">
                <h2 class="mb-3 text-5xl font-bold text-black dark:text-white">Chain Setup</h2>
                <p class="text-title-sm2 mb-10">Scan the network to find nodes</p>
                <button @click="newScan"
                    class="bg-primary text-black rounded-md e-flex items-center justify-center gap-2.5 py-4 px-2 sm:px-10 text-center font-medium hover:bg-opacity-90 lg:px-8 xl:px-10">
                    <div class="flex gap-1">
                        <span>Scan Now</span>
                        <span>
                            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5"
                                stroke="currentColor" width="20" height="20">
                                <path stroke-linecap="round" stroke-linejoin="round"
                                    d="m21 21-5.197-5.197m0 0A7.5 7.5 0 1 0 5.196 5.196a7.5 7.5 0 0 0 10.607 10.607Z" />
                            </svg>
                        </span>
                    </div>
                </button>
                <div v-if="errorMsg" class="relative mt-10 mb-10">
                    <h2>{{ errorMsg }}</h2>
                </div>
            </div>
        </div>
    </WizardLayout>
</template>

<style></style>