<script setup lang="ts">
import ButtonDefault from '@/components/Buttons/ButtonDefault.vue';
import { useRouter } from 'vue-router';

const router = useRouter();
</script>
<template>
    <div class="flex items-center justify-between space-x-4.5 ">
        <div>
            <div class="flex justify-center w-full">
                <label for="formCheckboxAll" class="flex items-center cursor-pointer">
                    <p>Select All</p>
                    <div class="relative">
                        <input type="checkbox" id="formCheckboxAll" class="taskCheckbox sr-only" />
                        <div
                            class="box ml-3 flex h-6 w-6 items-center justify-center rounded border  border-form-strokedark bg-form-input">
                            <span class="text-white opacity-0">
                                <svg class="fill-current" width="14" height="11" viewBox="0 0 10 7" fill="#50DBD9"
                                    xmlns="http://www.w3.org/2000/svg">
                                    <path fill-rule="evenodd" clip-rule="evenodd"
                                        d="M9.70685 0.292804C9.89455 0.480344 10 0.734667 10 0.999847C10 1.26503 9.89455 1.51935 9.70685 1.70689L4.70059 6.7072C4.51283 6.89468 4.2582 7 3.9927 7C3.72721 7 3.47258 6.89468 3.28482 6.7072L0.281063 3.70701C0.0986771 3.5184 -0.00224342 3.26578 3.785e-05 3.00357C0.00231912 2.74136 0.10762 2.49053 0.29326 2.30511C0.4789 2.11969 0.730026 2.01451 0.992551 2.01224C1.25508 2.00996 1.50799 2.11076 1.69683 2.29293L3.9927 4.58607L8.29108 0.292804C8.47884 0.105322 8.73347 0 8.99896 0C9.26446 0 9.51908 0.105322 9.70685 0.292804Z"
                                        fill="#50DBD9" />
                                </svg>
                            </span>
                        </div>
                    </div>

                </label>
            </div>
        </div>

        <div>
            <div class="gap-x-4 w-full md:w-fit  md:pt-0 space-x-3">
                <ButtonDefault route="/" customClasses="bg-black text-white rounded-md w-auto md:min-w-50 h-12 px-4">
                    <span class="flex gap-x-2">
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5"
                            stroke="currentColor" width="20" height="20">
                            <path stroke-linecap="round" stroke-linejoin="round"
                                d="M15.75 9V5.25A2.25 2.25 0 0 0 13.5 3h-6a2.25 2.25 0 0 0-2.25 2.25v13.5A2.25 2.25 0 0 0 7.5 21h6a2.25 2.25 0 0 0 2.25-2.25V15m3 0 3-3m0 0-3-3m3 3H9" />
                        </svg>
                        <span class="hidden md:block">Exit</span>
                    </span>
                </ButtonDefault>
                <ButtonDefault route="/wizard/step2"
                    customClasses="bg-black text-white rounded-md w-auto md:min-w-50 h-12 px-4">
                    <span class="flex gap-x-2">
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5"
                            stroke="currentColor" width="20" height="20">
                            <path stroke-linecap="round" stroke-linejoin="round"
                                d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0 3.181 3.183a8.25 8.25 0 0 0 13.803-3.7M4.031 9.865a8.25 8.25 0 0 1 13.803-3.7l3.181 3.182m0-4.991v4.99" />
                        </svg>
                        <span class="hidden md:block">Reload</span>
                    </span>
                </ButtonDefault>
                <ButtonDefault route="/dashboard/home" label=""
                    customClasses="bg-primary text-black rounded-md w-auto md:min-w-50 h-12 px-4">
                    <span class="flex gap-x-2">
                        <span class="hidden md:block">Continue</span>
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5"
                            stroke="currentColor" width="20" height="20">
                            <path stroke-linecap="round" stroke-linejoin="round"
                                d="M13.5 4.5 21 12m0 0-7.5 7.5M21 12H3" />
                        </svg>

                    </span>
                </ButtonDefault>
            </div>
        </div>
    </div>
</template>