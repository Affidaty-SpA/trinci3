<script lang="ts" setup>
import { DocumentDuplicateIcon } from '@heroicons/vue/24/outline'
import { CheckIcon } from '@heroicons/vue/24/solid';
import { ref } from 'vue';
const props = defineProps({
  data: {
    type: String,
    required: true,
    default: ""
  },
  class: String
})
const emits = defineEmits<{
  (event: 'copyToClipboard'): void
}>()
const copyDone = ref(false)

const onCopy = () => {
  navigator.clipboard.writeText(props.data)
  copyDone.value = true
  emits('copyToClipboard')
  setTimeout(() => {
    copyDone.value = false
  }, 800)
}
</script>
<template>
<DocumentDuplicateIcon v-if="!copyDone" class="cursor-pointer h-6 w-6 inline-flex  text-affigreen hover:text-emerald-500" :class="props.class" @click="onCopy()"/>
<CheckIcon v-else class="h-6 w-6 inline-flex opacity-50 dark:opacity-100 text-affigreen" :class="props.class"/>
</template>