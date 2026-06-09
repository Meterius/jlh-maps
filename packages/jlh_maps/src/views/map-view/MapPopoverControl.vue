<template>
  <UPopover modal>
    <template #default="{ open }">
      <UButton
        color="neutral"
        active-color="primary"
        variant="outline-solid"
        :active="open"
        size="xl"
        class="pointer-events-auto cursor-pointer"
        :icon="icon"
        :title="title"
        :aria-label="ariaLabel ?? title"
      />
    </template>

    <template #content>
      <slot />
    </template>
  </UPopover>
</template>

<script setup lang="ts">
defineProps<{
  icon: string
  title: string
  ariaLabel?: string
}>()
</script>
