<script setup lang="ts">
import type { INodeData } from '@/models';
import { PlusCircleIcon, XCircleIcon } from '@heroicons/vue/24/outline'
import { ref } from 'vue';
import { useNodeStore } from "@/stores/node";
import { storeToRefs } from 'pinia';

const { errorMsg } = storeToRefs(useNodeStore());
const { addNodeToNtw } = useNodeStore();
const nodeModel = ref<{ip: string, rest: number}>({} as {ip: string, rest: number});

const addNode = () => {
    if (nodeModel.value.ip === "" || nodeModel.value.rest === 0) return;
    if (!validateNodeObject(nodeModel.value).isValid) {
        errorMsg.value = validateNodeObject(nodeModel.value).errorMessage;
        return;
    } else {
        addNodeToNtw(nodeModel.value);
        clearNodeModel();
    }
}

const clearNodeModel = () => {
    nodeModel.value = {} as INodeData;
    errorMsg.value = undefined;
}

const validateNodeObject = (obj: {ip: string, rest: number}): { isValid: boolean, errorMessage?: string } => {
    // Regular expression for IP address validation
    const ipRegex = /^((25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?).){3}(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$/;

    // Regular expression for port number validation
    const portRegex = /^(6553[0-5]|655[0-2]\d|65[0-4]\d{2}|6[0-4]\d{3}|[1-5]\d{4}|\d{1,4})$/;

    // Check if IP is valid
    if (!ipRegex.test(obj.ip)) {
        return {
            isValid: false,
            errorMessage: "IP address not valid"
        };
    }

    // Check if rest is a valid port number
    if (!portRegex.test(String(obj.rest))) {
        return {
            isValid: false,
            errorMessage: "REST port not valid"
        };
    }

    // Check if socket is a valid port number
    // if (!portRegex.test(String(obj.socket))) {
    //     return {
    //         isValid: false,
    //         errorMessage: "Socket port not valid"
    //     };
    // }

    return {
        isValid: true
    };
}

</script>
<template>
    <div class="flex items-center justify-between space-x-4.5 ">
        <span v-if="errorMsg" class="text-xs font-semibold text-rose-500">{{ errorMsg }}</span>
        <div class="relative w-full flex gap-2">
            <input
                v-model="nodeModel.ip"
                type="text"
                placeholder="Node IP"
                class="h-13 w-full rounded-lg border border-stroke bg-gray px-5 font-medium text-black placeholder-body outline-none focus:border-primary dark:focus:border-primary dark:border-form-strokedark dark:bg-boxdark-2 dark:text-white"
            />
            <input
                v-model="nodeModel.rest"
                type="text"
                placeholder="Node REST port"
                class="h-13 w-full rounded-lg border border-stroke bg-gray px-5 font-medium text-black placeholder-body outline-none focus:border-primary dark:focus:border-primary dark:border-form-strokedark dark:bg-boxdark-2 dark:text-white"
            />
        </div>
        <button @click="addNode" class="bg-black text-white rounded-md w-auto !px-4 h-12">
            <div class="flex gap-1">
                <span class="hidden md:block text-sm">Add</span>
                <span class="flex gap-x-2 items-center">
                    <PlusCircleIcon class="h-6 w-6" />
                </span>
            </div>
        </button>
        <button @click="clearNodeModel" class="bg-black text-white rounded-md w-auto !px-4 h-12">
            <div class="flex gap-1">
                <span class="hidden md:block text-sm">Clear</span>
                <span class="flex gap-x-2 items-center">
                    <XCircleIcon class="h-6 w-6" />
                </span>
            </div>
        </button>
    </div>
</template>