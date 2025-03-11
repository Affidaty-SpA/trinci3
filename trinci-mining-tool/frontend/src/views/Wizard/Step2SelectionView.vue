<script setup lang="ts">
import WizardLayout from '@/layouts/WizardLayout.vue';
import NodeFound from '@/components/Nodes/NodeFound.vue';
import BottomBarLayout from '@/layouts/BottomBarLayout.vue';
import SearchNodes from '@/components/Forms/SearchNodes.vue';
import { storeToRefs } from 'pinia';
import { useNodeStore } from '@/stores/node';
import type { INodeData } from '@/models';
import { ArrowRightStartOnRectangleIcon, ArrowPathIcon, ArrowRightIcon } from '@heroicons/vue/24/solid';
import { useUserStore } from '@/stores/user.store';
import { useAuthIn } from '@/utils/useAuthin';
import { useRouter } from 'vue-router';
import WSClient from '@/utils/wsClient';
import { onMounted, ref } from 'vue';
import { EphemeralTx } from '@/utils/trinci';
import { useRootStore } from '@/stores/index.store';

const { nodesData } = storeToRefs(useNodeStore());
const { clearStorageAndLogout } = useUserStore();
const { scanNetwork, setErrorMsg, setActiveNodes } = useNodeStore();
const { setShowLoader } = useRootStore()
const { logout } = useAuthIn();
const router = useRouter();
const selectAll = ref(false);

const refreshScan = () => {
    setShowLoader(true)
    nodesData.value = nodesData.value.filter(item => item.source !== "scan");
    scanNetwork().finally(() => {
      setShowLoader(false)
    })
    .catch((e) => {
      setErrorMsg(e)
    })
}

const handleSelectAll = () => {
    nodesData.value = nodesData.value.map((node) => {
        node.selected = selectAll.value
        return node;
    })
}

const getNodes4Mining = async () => {
    const r = nodesData.value.reduce((acc, curr) => {
        if (curr.selected) {
            const { selected, source, ...others } = curr;
            acc.push(others);
        }
        return acc;
    }, [] as INodeData[]);

    return new EphemeralTx("#miningTool", "init" , r).initRemoteProcedureCall().then((response) => {
        console.log('submitRemoteProcedureCall response ==> ', response);
        // save response in store ["accountId", boolean][]

        // setActiveNodes([["QmSYfmxWXCLjeMQ4fqtuCg9KFmxq42jqRJmjE9Pghr4qhP", true], ["QmWhNHdc4PR9ZHPcUaZXsbajAwUioETvNBG8Q3v2p2cvNL", true]]);
        // setActiveMiningNodes(response.filter(item => item[1]))

        router.replace({name: "home"});
    }).catch(console.error);
}

const logoutTrigger = () => {
  logout();
  clearStorageAndLogout();
  router.replace({ name: 'login' });
  WSClient.disconnect();
}

onMounted(() => {
    if (nodesData.value.length === 0) {
        router.replace({ name: "step1setup" });
    } else {
        setShowLoader(true);
    }
})
</script>

<template>
    <WizardLayout>
        <div class="pb-24">
            <p class="text-sm sm:hidden font-medium text-gray-900">Nodes Found...</p>
            <div class="mt-6" aria-hidden="true">
                <div class="overflow-hidden rounded-full bg-black">
                    <div class="h-2 rounded-full bg-primary" style="width:60%"></div>
                </div>
                <div class="mt-6 hidden grid-cols-3 text-sm font-medium text-gray-600 sm:grid">
                    <div class=" text-primary">Chain Setup</div>
                    <div class="text-center">Nodes Found</div>
                    <div class="text-right">Chain Updated!</div>
                </div>
            </div>
            <div class="mt-12 pb-24 lg:mt-20 text-center">
                <h2 class="mb-3 text-5xl font-bold text-black dark:text-white">
                    Nodes found!
                </h2>
                <p class="text-title-sm2 mb-10">
                    Select the ones you want to add to your network.
                </p>
                <div class="grid grid-cols-1 gap-7.5 sm:grid-cols-2 xl:grid-cols-3">
                    <NodeFound v-for="(node, i) of nodesData.filter(item => item.source == 'scan')" :key="i" :node-object="node">
                        <input v-model="node.selected" type="checkbox" class="taskCheckbox cursor-pointer flex h-6 w-6 items-center justify-center" />
                    </NodeFound>
                    <NodeFound v-for="(node, i) of nodesData.filter(item => item.source != 'scan')" :key="i" :node-object="node">
                        <input v-model="node.selected" type="checkbox" class="taskCheckbox cursor-pointer flex h-6 w-6 items-center justify-center" />
                    </NodeFound>
                </div>
            </div>
            <BottomBarLayout>
                <div>
                    <SearchNodes :node-list="nodesData" />
                </div>
                <div class="md:flex w-full gap-x-4 justify-between">
                    <div class="flex items-center">
                        <div class="flex justify-center w-full">
                            <label for="formCheckboxAll" class="flex items-center cursor-pointer">
                                <div class="relative">
                                    <input v-model="selectAll" @change="handleSelectAll" type="checkbox" id="formCheckboxAll" class="taskCheckbox cursor-pointer" />
                                    <!-- <div
                                        class="box flex h-6 w-6 items-center justify-center rounded border  border-form-strokedark bg-form-input">
                                        <span class="text-white opacity-0">
                                            <svg class="fill-current" width="14" height="11" viewBox="0 0 10 7"
                                                fill="#50DBD9" xmlns="http://www.w3.org/2000/svg">
                                                <path fill-rule="evenodd" clip-rule="evenodd"
                                                    d="M9.70685 0.292804C9.89455 0.480344 10 0.734667 10 0.999847C10 1.26503 9.89455 1.51935 9.70685 1.70689L4.70059 6.7072C4.51283 6.89468 4.2582 7 3.9927 7C3.72721 7 3.47258 6.89468 3.28482 6.7072L0.281063 3.70701C0.0986771 3.5184 -0.00224342 3.26578 3.785e-05 3.00357C0.00231912 2.74136 0.10762 2.49053 0.29326 2.30511C0.4789 2.11969 0.730026 2.01451 0.992551 2.01224C1.25508 2.00996 1.50799 2.11076 1.69683 2.29293L3.9927 4.58607L8.29108 0.292804C8.47884 0.105322 8.73347 0 8.99896 0C9.26446 0 9.51908 0.105322 9.70685 0.292804Z"
                                                    fill="#50DBD9" />
                                            </svg>
                                        </span>
                                    </div> -->
                                </div>
                                <p class="ml-3">Select All</p>
                            </label>
                        </div>
                    </div>
                    <div class="gap-x-4 w-full md:w-fit  pt-3 md:pt-0 space-y-3 md:space-x-3">
                        <button
                            @click="logoutTrigger"
                            class="w-full md:w-auto inline-flex items-center justify-center gap-2.5 py-4 px-2 sm:px-10 text-center font-medium hover:bg-opacity-90 lg:px-8 xl:px-10 bg-black text-white rounded-md md:min-w-50">
                            <div class="flex gap-2 items-center">
                                <span>Exit</span>
                                <span>
                                    <ArrowRightStartOnRectangleIcon class="h-5 w-5" />
                                </span>
                            </div>
                        </button>
                        <button
                            @click="refreshScan"
                            class="w-full md:w-auto inline-flex items-center justify-center gap-2.5 py-4 px-2 sm:px-10 text-center font-medium hover:bg-opacity-90 lg:px-8 xl:px-10 bg-black text-white rounded-md md:min-w-50">
                            <div class="flex gap-2 items-center">
                                <span>Reload</span>
                                <span>
                                  <ArrowPathIcon class="h-5 w-5" />
                                </span>
                            </div>
                        </button>
                        <button
                            @click="getNodes4Mining()"
                            class="bg-primary text-black w-full md:w-auto inline-flex items-center justify-center gap-2.5 py-4 px-2 sm:px-10 text-center font-medium hover:bg-opacity-90 lg:px-8 xl:px-10 rounded-md md:min-w-50">
                            <div class="flex gap-2 items-center">
                                <span>Continue</span>
                                <span>
                                   <ArrowRightIcon class="h-5 w-5" />
                                </span>
                            </div>
                        </button>
                    </div>
                </div>
            </BottomBarLayout>
        </div>
    </WizardLayout>
</template>

<style></style>