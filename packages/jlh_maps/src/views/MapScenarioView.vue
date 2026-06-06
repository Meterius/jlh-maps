<template>
  <MapView v-if="scenario" :scenario-name="scenario.name" />
  <div v-else class="grid min-h-screen place-items-center p-6 text-center">
    <div class="grid gap-2">
      <h1 class="text-xl font-semibold">Unknown map scenario</h1>
      <p class="text-muted">
        Scenario <span class="font-mono">{{ name }}</span> is not registered.
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import MapView from '@/views/MapView.vue'
import { getMapViewScenario } from '@/views/map-view/map-view-scenario'

const props = defineProps<{
  name: string
}>()

const scenario = computed(() => getMapViewScenario(props.name))
</script>
