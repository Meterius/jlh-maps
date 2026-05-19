<template>
  <div :class="ui.root" role="radiogroup" :aria-label="props.ariaLabel">
    <UCard
      :ui="{
        body: ui.cardBody,
      }"
      :class="ui.card"
    >
      <UButton
        v-for="option in props.options"
        :key="'button-' + option.value"
        type="button"
        :color="model === option.value ? 'primary' : 'neutral'"
        :variant="model === option.value ? 'solid' : 'ghost'"
        size="sm"
        block
        role="radio"
        :class="[ui.button, model === option.value ? ui.buttonActive : ui.buttonInactive]"
        :aria-checked="model === option.value"
        :title="option.label"
        @click="model = option.value"
      >
        <span :class="ui.buttonContent">
          <UIcon v-if="option.icon" :name="option.icon" :class="ui.icon" />
          <span :class="ui.label">{{ option.label }}</span>
        </span>
      </UButton>
    </UCard>

    <div v-if="enableSubLabels" :class="ui.subLabelRow">
      <div
        v-for="option in props.options"
        :key="'badge-' + option.value"
        :class="ui.subLabelCell"
      >
        <Transition
          enter-active-class="transition duration-150 ease-out"
          enter-from-class="-translate-y-1 opacity-0"
          enter-to-class="translate-y-0 opacity-100"
          leave-active-class="transition duration-100 ease-in"
          leave-from-class="translate-y-0 opacity-100"
          leave-to-class="-translate-y-1 opacity-0"
        >
          <UBadge
            v-if="option.subLabel"
            :label="option.subLabel"
            color="neutral"
            variant="soft"
            size="sm"
            :class="ui.subLabelBadge"
          />
        </Transition>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts" generic="T extends string">
import { computed } from 'vue'
import type { ClassValue } from 'tailwind-variants'

export type ModeSelectorOption<T extends string = string> = {
  value: T
  label: string
  icon?: string
  subLabel?: string
}

export type ModeSelectorUI = Partial<
  Record<
    | 'root'
    | 'card'
    | 'cardBody'
    | 'button'
    | 'buttonActive'
    | 'buttonInactive'
    | 'buttonContent'
    | 'icon'
    | 'label'
    | 'subLabelRow'
    | 'subLabelCell'
    | 'subLabelBadge',
    ClassValue
  >
>

const props = withDefaults(
  defineProps<{
    options: readonly ModeSelectorOption<T>[]
    enableSubLabels?: boolean
    ariaLabel?: string
    ui?: ModeSelectorUI
  }>(),
  {
    ariaLabel: 'Mode',
  },
)

const model = defineModel<T>({ required: true })

const ui = computed(() => ({
  root: ['relative isolate', props.ui?.root],
  card: ['relative z-10', props.ui?.card],
  cardBody: ['flex !p-0 sm:!p-0', props.ui?.cardBody],
  button: ['rounded-none py-3', props.ui?.button],
  buttonActive: props.ui?.buttonActive,
  buttonInactive: ['cursor-pointer text-muted', props.ui?.buttonInactive],
  buttonContent: ['inline-flex min-w-0 items-center gap-1.5', props.ui?.buttonContent],
  icon: ['size-4 shrink-0', props.ui?.icon],
  label: ['truncate', props.ui?.label],
  subLabelRow: ['relative z-0 -mt-px flex h-6 overflow-hidden', props.ui?.subLabelRow],
  subLabelCell: ['flex min-w-0 flex-1 justify-center', props.ui?.subLabelCell],
  subLabelBadge: [
    'pointer-events-none max-w-[calc(100%-0.5rem)] self-center rounded-t-none font-normal',
    props.ui?.subLabelBadge,
  ],
}))
</script>
