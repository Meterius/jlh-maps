<template>
  <div class="min-w-0 max-w-full p-4">
    <div v-if="loading" class="grid min-w-0 gap-1">
      <div v-for="idx in 3" :key="idx" class="grid grid-cols-[1.5rem_minmax(0,1fr)] items-center gap-3 p-2">
        <USkeleton class="size-5 rounded-full" />
        <USkeleton class="h-4 w-full max-w-56" />
      </div>
    </div>

    <div v-else class="grid min-w-0 gap-1">
      <template v-for="item in interpretedItems" :key="item.key">
        <UCollapsible
          v-if="item.openingHours"
          v-model:open="openingHoursOpen"
          class="min-w-0"
        >
          <InterpretedPropertyButton
            :class="openingHoursOpen ? 'rounded-b-none' : undefined"
            :icon="item.icon"
            :trailing-icon="openingHoursOpen ? 'lucide:chevron-up' : 'lucide:chevron-down'"
            :to="item.href"
            :target="item.href?.startsWith('http') ? '_blank' : undefined"
            @click="item.onClick"
          >
            <template #label>
              <span :class="item.openingHours.statusClass">
                {{ item.openingHours.statusLabel }}
              </span>
              <span v-if="item.openingHours.nextLabel" class="text-highlighted">
                · {{ item.openingHours.nextLabel }}
              </span>
            </template>
          </InterpretedPropertyButton>

          <template #content>
            <div
              class="grid min-w-0 gap-2 rounded-b-md border border-t-0 border-default px-4 pb-3 pt-2 text-md"
            >
              <div
                v-for="day in item.openingHours.weekDays"
                :key="day.key"
                :class="[
                  'grid min-w-0 grid-cols-[5rem_minmax(0,1fr)] gap-3',
                  day.isToday ? 'font-semibold text-highlighted' : 'text-muted',
                ]"
              >
                <span>{{ day.dayLabel }}</span>
                <span class="min-w-0 truncate text-right">{{ day.hoursLabel }}</span>
              </div>
            </div>
          </template>
        </UCollapsible>

        <InterpretedPropertyButton
          v-else
          :icon="item.icon"
          :to="item.href"
          :target="item.href?.startsWith('http') ? '_blank' : undefined"
          @click="item.onClick"
        >
          <template #label>{{ item.label }}</template>
        </InterpretedPropertyButton>
      </template>

      <p v-if="interpretedItems.length === 0" class="px-2 py-3 text-sm text-muted">
        No details available.
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { GeoJSONFeature } from 'maplibre-gl'
import OpeningHours from 'opening_hours'
import { useNow } from '@vueuse/core'
import type { OsmData } from '@/external/endpoints.ts'
import InterpretedPropertyButton from '@/components/map-slideover/details/InterpretedPropertyButton.vue'

const props = defineProps<{
  feature?: GeoJSONFeature
  osmData?: OsmData | null
  loading?: boolean
}>()

const now = useNow({ interval: 60_000 })
const openingHoursOpen = ref(false)

type InterpretedItem = {
  key: string
  icon: string
  label: string
  href?: string
  onClick?: () => void
  openingHours?: OpeningHoursInterpretation
}

type OpeningHoursInterpretation = {
  statusLabel: string
  statusClass: string
  nextLabel?: string
  weekDays: OpeningHoursWeekDay[]
}

type OpeningHoursWeekDay = {
  key: string
  dayLabel: string
  hoursLabel: string
  isToday: boolean
}

const normalizedTags = computed<Record<string, unknown>>(() => ({
  ...(props.feature?.properties ?? {}),
  ...(props.osmData?.tags ?? {}),
}))

const interpretedItems = computed<InterpretedItem[]>(() => {
  const items: InterpretedItem[] = []
  const openingHours = getStringValue('opening_hours')
  const openingHoursInterpretation = interpretOpeningHours(openingHours, now.value)
  const website = normalizeWebsite(getStringValue('website', 'contact:website', 'url'))
  const telephone = getStringValue('phone', 'contact:phone', 'telephone')
  const address = getAddress()

  if (openingHours) {
    items.push({
      key: 'opening-hours',
      icon: 'lucide:clock',
      label: openingHoursInterpretation?.statusLabel ?? openingHours,
      openingHours: openingHoursInterpretation,
      onClick: openingHoursInterpretation ? undefined : () => copyText(openingHours),
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

const interpretOpeningHours = (value: string | undefined, date: Date) => {
  if (!value) return undefined

  try {
    const openingHours = new OpeningHours(value)
    const state = getOpeningHoursState(openingHours, date)
    const nextChange = openingHours.getNextChange(date)
    const weekDays = getOpeningHoursWeekDays(openingHours, date)

    if (state === 'unknown') {
      return {
        statusLabel: 'Hours unknown',
        statusClass: 'text-warning',
        nextLabel: nextChange ? `Next update ${formatRelativeDateTime(nextChange, date)}` : undefined,
        weekDays,
      }
    }

    if (state === 'open') {
      return {
        statusLabel: 'Open',
        statusClass: 'text-success',
        nextLabel: nextChange ? `Closes ${formatRelativeDateTime(nextChange, date)}` : 'Open now',
        weekDays,
      }
    }

    return {
      statusLabel: 'Closed',
      statusClass: 'text-error',
      nextLabel: nextChange ? `Opens ${formatRelativeDateTime(nextChange, date)}` : undefined,
      weekDays,
    }
  } catch {
    return undefined
  }
}

const getOpeningHoursState = (openingHours: OpeningHours, date: Date) => {
  if (openingHours.getUnknown(date)) return 'unknown'

  return openingHours.getState(date) ? 'open' : 'closed'
}

const getOpeningHoursWeekDays = (openingHours: OpeningHours, date: Date): OpeningHoursWeekDay[] => {
  const today = startOfLocalDay(date)

  return Array.from({ length: 7 }, (_, index) => {
    const dayStart = addLocalDays(today, index)
    const dayEnd = addLocalDays(dayStart, 1)
    const intervals = openingHours.getOpenIntervals(dayStart, dayEnd)
    const isToday = index === 0

    return {
      key: dayStart.toISOString(),
      dayLabel: getWeekDayLabel(dayStart, isToday),
      hoursLabel: formatDayIntervals(intervals, dayStart, dayEnd),
      isToday,
    }
  })
}

const formatDayIntervals = (
  intervals: [Date, Date, boolean, string | undefined][],
  dayStart: Date,
  dayEnd: Date,
) => {
  if (intervals.length === 0) return 'Closed'

  return intervals
    .map(([rawStart, rawEnd, isUnknown]) => {
      const start = rawStart < dayStart ? dayStart : rawStart
      const end = rawEnd > dayEnd ? dayEnd : rawEnd
      const prefix = isUnknown ? 'Maybe ' : ''

      if (isSameTime(start, dayStart) && isSameTime(end, dayEnd)) return `${prefix}Open 24 hours`

      return `${prefix}${formatTime(start)}-${formatTime(end)}`
    })
    .join(', ')
}

const formatTime = (value: Date) =>
  new Intl.DateTimeFormat(undefined, {
    hour: 'numeric',
    minute: '2-digit',
  }).format(value)

const getWeekDayLabel = (value: Date, isToday: boolean) => {
  if (isToday) return 'Today'

  return new Intl.DateTimeFormat(undefined, {
    weekday: 'long',
  }).format(value)
}

const startOfLocalDay = (value: Date) =>
  new Date(value.getFullYear(), value.getMonth(), value.getDate())

const addLocalDays = (value: Date, days: number) => {
  const next = new Date(value)
  next.setDate(next.getDate() + days)
  return next
}

const isSameTime = (left: Date, right: Date) => left.getTime() === right.getTime()

const formatRelativeDateTime = (value: Date, nowDate: Date) => {
  const time = formatTime(value)

  if (isSameLocalDate(value, nowDate)) return `at ${time}`

  const tomorrow = new Date(nowDate)
  tomorrow.setDate(tomorrow.getDate() + 1)
  if (isSameLocalDate(value, tomorrow)) return `tomorrow at ${time}`

  const day = new Intl.DateTimeFormat(undefined, {
    weekday: 'short',
  }).format(value)

  return `${day} at ${time}`
}

const isSameLocalDate = (left: Date, right: Date) =>
  left.getFullYear() === right.getFullYear() &&
  left.getMonth() === right.getMonth() &&
  left.getDate() === right.getDate()

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
