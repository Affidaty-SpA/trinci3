<script setup lang="ts">
import { useNodeStore } from "@/stores/node";
import { storeToRefs } from "pinia";
import CopyToClipboard from "@/components/CopyToClipboard.vue";
import { chop } from "@/utils/utils";

const { nodeData, currentAccountId } = storeToRefs(useNodeStore());

</script>
<template>
    <div
    class="col-span-12 rounded-xl border border-stroke bg-white px-5 pb-5 pt-7.5 shadow-default dark:border-strokedark dark:bg-boxdark sm:px-7.5 xl:col-span-7"
    >
        <div class="mb-5.5 flex flex-wrap items-center justify-between gap-2">
            <div>
                <h4 class="text-title-sm2 font-bold text-black dark:text-white">Config (VISA)</h4>
            </div>
        </div>
        <ul class="max-h-[350px] overflow-y-auto border-b border-b-mining-gray-2">
            <li class="flex justify-between items-center mb-3">
                <div class=" text-mining-gray-3">node account id</div>
                <div class="flex items-center">
                    <span class="truncate inline-block w-30 text-right">{{ chop(currentAccountId, 5, 5, '...') }}</span>
                    <CopyToClipboard :data="currentAccountId" class="px-2 h-10 w-10" />
                </div>
            </li>
            <li class="flex justify-between items-center mb-3" v-for="node of Object.entries(nodeData?.connection || {})">
                <div class=" text-mining-gray-3">{{ node[0] }}</div>
                <div class="flex items-center">
                    <span class="truncate inline-block w-30 text-right">{{ node[1] }}</span>
                </div>
            </li>
        </ul>
        <!-- <div class="text-right mt-5">
            <button>View all</button>
        </div> -->
    </div>
</template>