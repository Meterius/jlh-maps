<template>
  <UApp>
    <UMain>
      <div
        v-if="warningMessage"
        class="absolute top-0 left-0 right-0 z-1000 transition-top duration-200 ease-out"
      >
        <UBanner variant="outline" color="warning" :title="warningMessage" close />
      </div>

      <RouterView />
    </UMain>
  </UApp>
</template>

<script setup lang="ts">
import { RouterView } from 'vue-router'
import { Browser, useBrowser } from '@/composables/browser.ts'
import { computed } from 'vue'

const { browser } = useBrowser()

const warningMessage = computed(() => {
  if (browser.value !== Browser.Chrome) {
    return `Application works best with Chrome. You are currently using ${browser.value} which may cause slow-downs or incompatibilities.`
  }

  return null
})
</script>
