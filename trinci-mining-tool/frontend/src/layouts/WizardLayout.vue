<script setup lang="ts">
</script>

<template>
    <section>
        <div class="mx-auto max-w-7xl px-6 lg:px-8 pt-8 lg:pt-16">
            <slot></slot>
            
        </div>
    </section>
</template>

<style scoped>
@import 'tailwindcss/components';

section {
    @apply  bg-center bg-cover h-screen overflow-y-auto;
    background-image: url(@/assets/images/illustration/bg-login.svg);
}
</style>
