<script setup lang="ts">
import type { INodeData } from '@/models';
import { chop } from '@/utils/utils';
import { toRefs } from 'vue';

const props = defineProps<{
    nodeObject: INodeData;
}>()

const { nodeObject } = toRefs(props);

</script>

<template>
    <div class="rounded-md border border-stroke bg-white shadow-default dark:border-[#707070] dark:bg-[#3D4B5D]">
        <div class="px-5.5 pt-4 pb-4.5">
            <div class="flex justify-between mb-4 items-center">
                <label for="formCheckboxNodes" class="flex cursor-pointer">
                    <div class="relative pt-0.5 flex">
                        <slot></slot>
                        <div class="text-left ml-3">
                            <p class="font-medium text- text-xs md:text-sm truncate">
                                ACCOUNT ID: {{ chop(nodeObject.accountId, 10, 10 , '...') }}
                            </p>
                        </div>
                        <!-- <div
                            class="box flex h-6 w-6 items-center justify-center rounded border  border-form-strokedark bg-form-input">
                            <span class="text-white opacity-0">
                                <svg class="fill-current" width="14" height="11" viewBox="0 0 10 7" fill="#50DBD9"
                                    xmlns="http://www.w3.org/2000/svg">
                                    <path fill-rule="evenodd" clip-rule="evenodd"
                                        d="M9.70685 0.292804C9.89455 0.480344 10 0.734667 10 0.999847C10 1.26503 9.89455 1.51935 9.70685 1.70689L4.70059 6.7072C4.51283 6.89468 4.2582 7 3.9927 7C3.72721 7 3.47258 6.89468 3.28482 6.7072L0.281063 3.70701C0.0986771 3.5184 -0.00224342 3.26578 3.785e-05 3.00357C0.00231912 2.74136 0.10762 2.49053 0.29326 2.30511C0.4789 2.11969 0.730026 2.01451 0.992551 2.01224C1.25508 2.00996 1.50799 2.11076 1.69683 2.29293L3.9927 4.58607L8.29108 0.292804C8.47884 0.105322 8.73347 0 8.99896 0C9.26446 0 9.51908 0.105322 9.70685 0.292804Z"
                                        fill="#50DBD9" />
                                </svg>
                            </span>
                        </div> -->

                    </div>
                </label>
            </div>
            <div class="grid grid-cols-3">
                <div class="text-left">
                    <p class="font-medium text-xs md:text-sm">
                        IP: {{ nodeObject.ip }}
                    </p>
                </div>
                <div class="text-center">
                    <p class="font-medium text-xs md:text-sm ">
                        REST: {{ nodeObject.rest }}
                    </p>
                </div>
                <div class="text-right">
                    <p class="font-medium text-xs md:text-sm">
                        SOCKET: {{ nodeObject.socket }}
                    </p>
                </div>
            </div>
        </div>
    </div>
</template>

<style scoped>
@import 'tailwindcss/components';

.disabled {
    @apply opacity-50;
}
</style>