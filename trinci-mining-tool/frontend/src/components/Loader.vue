<script lang="ts" setup>
import { storeToRefs } from 'pinia';
import { TransitionGroup, onMounted, onUnmounted, ref } from 'vue';
import { useRootStore } from "@/stores/index.store"
import loader from "../assets/loader.svg";

const { loaderMessage } = storeToRefs(useRootStore());

let interval = ref();

onMounted(() => {
  interval.value = setInterval(() => {
    loaderMessage.value.shift()
  }, 5000)
})

onUnmounted(() => {
  clearInterval(interval.value);
})

</script>
<template>
  <div class="flex flex-col w-full h-screen justify-center items-center px-5">
    <div class="fixed top-0 bottom-0 left-0 right-0 bg-white/80 dark:bg-black/80 z-[1996]"></div>
    <img class="fixed -top-1 bottom-0 -left-1 right-0 m-auto z-[1997] w-8" src="@/assets/images/icons/mining-symbol.svg" alt="Your Company" data-v-5b339c2c="">
    <img class="fixed top-0 bottom-0 left-0 right-0 m-auto z-[1997] w-24 h-24" :src="loader"></img>
    <div class="mt-[150px] z-[9999]">
      <TransitionGroup name="bounce" tag="ul" mode="out-in">
        <li
          v-if="loaderMessage"
          v-for="(message,i) in loaderMessage"
          :key="i"
          :class="{'text-normal text-center font-bold text-affigreen': true}" >
            {{ message }}
        </li>
      </TransitionGroup>
    </div>
  </div>

</template>
<style>
/* 1. declare transition */
.fade-move,
.fade-enter-active,
.fade-leave-active {
  transition: all 0.5s cubic-bezier(0.55, 0, 0.1, 1);
}

/* 2. declare enter from and leave to state */
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
  transform: scaleY(0.01) translate(30px, 0);
}

/* 3. ensure leaving items are taken out of layout flow so that moving
      animations can be calculated correctly. */
.fade-leave-active {
  position: absolute;
}

.bounce-enter-active {
  animation: bounce-in 0.5s;
}
.bounce-leave-active {
  animation: bounce-in 0.5s reverse;
}
@keyframes bounce-in {
  0% {
    transform: scale(0);
  }
  50% {
    transform: scale(1.25);
  }
  100% {
    transform: scale(1);
  }
}
</style>