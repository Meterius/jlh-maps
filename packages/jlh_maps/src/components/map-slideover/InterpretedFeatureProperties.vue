<template>
  <div class="min-w-0 max-w-full p-4">
    <div v-if="loading" class="grid min-w-0 gap-1">
      <div v-for="idx in 3" :key="idx" class="grid grid-cols-[1.5rem_minmax(0,1fr)] items-center gap-3 p-2">
        <USkeleton class="size-5 rounded-full" />
        <USkeleton class="h-4 w-full max-w-56" />
      </div>
    </div>

    <div v-else class="grid min-w-0 gap-1">
      <UButton
        v-for="item in interpretedItems"
        :key="item.key"
        class="justify-start px-2 py-2 text-left cursor-pointer"
        color="neutral"
        variant="outline"
        :icon="item.icon"
        :label="item.label"
        :to="item.href"
        :target="item.href?.startsWith('http') ? '_blank' : undefined"
        @click="item.onClick"
      >
        <template #default>
          <span class="grid min-w-0 gap-0.5 pl-2">
            <span class="truncate text-sm font-medium text-highlighted">{{ item.label }}</span>
          </span>
        </template>
      </UButton>

      <p v-if="interpretedItems.length === 0" class="px-2 py-3 text-sm text-muted">
        No details available.
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { GeoJSONFeature } from 'maplibre-gl'
import type { OsmData } from '@/external/endpoints.ts'

const props = defineProps<{
  feature?: GeoJSONFeature
  osmData?: OsmData | null
  loading?: boolean
}>()

type InterpretedItem = {
  key: string
  icon: string
  label: string
  href?: string
  onClick?: () => void
}

const normalizedTags = computed<Record<string, unknown>>(() => ({
  ...(props.feature?.properties ?? {}),
  ...(props.osmData?.tags ?? {}),
}))

const interpretedItems = computed<InterpretedItem[]>(() => {
  const items: InterpretedItem[] = []
  const openingHours = getStringValue('opening_hours')
  const website = normalizeWebsite(getStringValue('website', 'contact:website', 'url'))
  const telephone = getStringValue('phone', 'contact:phone', 'telephone')
  const address = getAddress()

  if (openingHours) {
    items.push({
      key: 'opening-hours',
      icon: 'lucide:clock',
      label: openingHours,
      onClick: () => copyText(openingHours),
    })
  }

  if (website) {
    items.push({
      key: 'website',
      icon: 'lucide:globe',
      label: getWebsiteLabel(website),
      href: website,
    })
  }

  if (telephone) {
    items.push({
      key: 'telephone',
      icon: 'lucide:phone',
      label: telephone,
      href: `tel:${telephone.replace(/\s+/g, '')}`,
    })
  }

  if (address) {
    items.push({
      key: 'address',
      icon: 'lucide:map-pin',
      label: address,
      href: `https://www.google.com/maps/search/?api=1&query=${encodeURIComponent(address)}`,
    })
  }

  return items
})

const getStringValue = (...keys: string[]) => {
  for (const key of keys) {
    const value = normalizedTags.value[key]
    if (typeof value === 'string' && value.trim()) return value.trim()
    if (typeof value === 'number') return String(value)
  }

  return undefined
}

const getAddress = () => {
  const directAddress = getStringValue('address')
  if (directAddress) return directAddress

  const street = getStringValue('addr:street')
  const houseNumber = getStringValue('addr:housenumber')
  const city = getStringValue('addr:city')
  const postcode = getStringValue('addr:postcode')
  const country = getStringValue('addr:country')

  const streetLine = [street, houseNumber].filter(Boolean).join(' ')
  const cityLine = [postcode, city].filter(Boolean).join(' ')

  return [streetLine, cityLine, country].filter(Boolean).join(', ') || undefined
}

const normalizeWebsite = (value?: string) => {
  if (!value) return undefined

  return /^https?:\/\//i.test(value) ? value : `https://${value}`
}

const getWebsiteLabel = (website: string) => {
  try {
    return new URL(website).hostname.replace(/^www\./i, '')
  } catch {
    return website.replace(/^https?:\/\//i, '').replace(/^www\./i, '').split('/')[0] ?? website
  }
}

const copyText = (value: string) => {
  navigator.clipboard?.writeText(value)
}
</script>
