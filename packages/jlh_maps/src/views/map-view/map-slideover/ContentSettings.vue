<template>
  <div class="grid w-full overflow-auto overflow-x-hidden">
    <div class="row">
      <div class="row p-4">
        <h5 class="pb-2">Debug</h5>
        <div class="grid gap-2">
          <label class="debug-toggle">
            <input v-model="showTileBoundaries" type="checkbox" />
            <span>Tile boundaries</span>
          </label>
          <label class="debug-toggle">
            <input v-model="showCollisionBoxes" type="checkbox" />
            <span>Collision boxes</span>
          </label>
          <label class="debug-toggle">
            <input v-model="showPadding" type="checkbox" />
            <span>Padding</span>
          </label>
        </div>
      </div>
    </div>
    <div class="row" v-if="useBevyRet">
      <USeparator />
      <div class="row p-4">
        <h5 class="pb-2">Bevy</h5>
        <div class="grid gap-2">
          <label class="debug-toggle">
            <input v-model="useBevyRet.mapViewSettings.value.enableBuildings" type="checkbox" />
            <span>Buildings</span>
          </label>
          <label class="debug-toggle">
            <input v-model="useBevyRet.mapViewSettings.value.enableWaters" type="checkbox" />
            <span>Water</span>
          </label>
          <label class="debug-toggle">
            <input v-model="useBevyRet.mapViewSettings.value.enableShadows" type="checkbox" />
            <span>Shadows</span>
          </label>
          <label class="debug-toggle">
            <input v-model="useBevyRet.mapViewSettings.value.enableWindowCameras" type="checkbox" />
            <span>Debug canvas</span>
          </label>
        </div>
      </div>
    </div>
    <div class="row" v-if="useBevyRet">
      <USeparator />
      <div class="row p-4">
        <h5 class="pb-2">Camera</h5>
        <div class="grid gap-2">
          <label class="debug-toggle">
            <input
              v-model="useBevyRet.mapViewCameraSettings.value.enableColorGrading"
              type="checkbox"
            />
            <span>Color grading</span>
          </label>
          <label class="debug-toggle">
            <input
              v-model="useBevyRet.mapViewCameraSettings.value.enableTonemapping"
              type="checkbox"
            />
            <span>Tonemapping</span>
          </label>
          <label class="debug-toggle">
            <input v-model="useBevyRet.mapViewCameraSettings.value.enableMsaa" type="checkbox" />
            <span>MSAA</span>
          </label>
          <label class="debug-toggle">
            <input v-model="useBevyRet.mapViewCameraSettings.value.enableSsao" type="checkbox" />
            <span>SSAO</span>
          </label>
          <label class="debug-toggle">
            <input v-model="useBevyRet.mapViewCameraSettings.value.enableTaa" type="checkbox" />
            <span>TAA</span>
          </label>
        </div>
      </div>
    </div>
    <div class="row">
      <USeparator />
      <div class="row p-4">
        <h5 class="pb-2">Layers</h5>
        <UTree
          ref="layerTree"
          :nested="false"
          :unmount-on-hide="false"
          :items="layerItems"
          class="max-h-[400px] w-full min-w-0 overflow-auto rounded-md border border-default"
          @select="$event.preventDefault()"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { MapLibreMap } from 'maplibre-gl'
import { computed, shallowRef, useTemplateRef, watch } from 'vue'
import type { TreeItem } from '@nuxt/ui'
import { useSortable } from '@vueuse/integrations/useSortable'
import { useBevy } from '@/bevy'
import { createDynamicComposable } from '@/composables/helper.ts'

const props = defineProps<{
  map: MapLibreMap
  bevyInstanceId?: string
}>()

const useBevyRet = createDynamicComposable(
  () => props.bevyInstanceId,
  (instanceId) => (instanceId !== undefined ? useBevy(instanceId) : null),
)

const showTileBoundaries = computed({
  get: () => props.map.showTileBoundaries,
  set: (value: boolean) => {
    // eslint-disable-next-line vue/no-mutating-props
    props.map.showTileBoundaries = value
  },
})

const showCollisionBoxes = computed({
  get: () => props.map.showCollisionBoxes,
  set: (value: boolean) => {
    // eslint-disable-next-line vue/no-mutating-props
    props.map.showCollisionBoxes = value
  },
})

const showPadding = computed({
  get: () => props.map.showPadding,
  set: (value: boolean) => {
    // eslint-disable-next-line vue/no-mutating-props
    props.map.showPadding = value
  },
})

const layerItems = shallowRef<TreeItem[]>(
  props.map.getLayersOrder().map((layer) => ({
    layer,
    label: layer,
    icon: 'i-vscode-icons-file-type-maplibre',
  })),
)

watch(layerItems, () => {
  layerItems.value.forEach((item) => {
    props.map.moveLayer(item.layer)
  })
})

const layerTree = useTemplateRef<HTMLElement>('layerTree')

useSortable(layerTree, layerItems, {
  animation: 150,
  ghostClass: 'opacity-50',
})
</script>

<style scoped>
.debug-toggle {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  width: fit-content;
  cursor: pointer;
}

.debug-toggle input {
  cursor: pointer;
}
</style>
