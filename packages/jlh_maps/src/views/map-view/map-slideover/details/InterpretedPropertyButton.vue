<template>
  <UButton
    v-bind="$attrs"
    class="justify-start px-2 py-2 text-left text-md cursor-pointer"
    block
    size="lg"
    color="neutral"
    variant="outline"
  >
    <template #default>
      <span class="grid min-w-0 gap-0.5 pl-2">
        <span class="truncate font-medium text-highlighted">
          <slot name="label" />
        </span>
      </span>
    </template>
  </UButton>
</template>

<script setup lang="ts">
defineOptions({
  inheritAttrs: false,
})
</script>
