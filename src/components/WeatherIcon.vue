<script setup lang="ts">
/**
 * WeatherIcon — filled, iPhone-style weather icons.
 *
 * Clouds are built from overlapping filled circles (reliable in any renderer).
 * Sun uses a filled disc + bold lines for rays.
 * Precipitation uses thick coloured lines below the cloud body.
 * No external package; no stroke-only hairlines that vanish at small sizes.
 */
import { computed } from 'vue'
import { classify } from '../composables/weatherUtils'

const props = defineProps<{ code: number }>()
const type  = computed(() => classify(props.code))
</script>

<template>
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    fill="none"
    class="wi"
    aria-hidden="true"
  >

    <!-- ── Clear ─────────────────────────────────────────────── -->
    <!-- Filled sun disc + 8 bold rays                           -->
    <template v-if="type === 'clear'">
      <circle cx="12" cy="12" r="5.5" fill="#FFCC00"/>
      <g stroke="#FFCC00" stroke-width="2.5" stroke-linecap="round">
        <line x1="12" y1="1"    x2="12" y2="3.5"/>
        <line x1="12" y1="20.5" x2="12" y2="23"/>
        <line x1="1"  y1="12"   x2="3.5" y2="12"/>
        <line x1="20.5" y1="12" x2="23"  y2="12"/>
        <line x1="4.22" y1="4.22"   x2="6.05" y2="6.05"/>
        <line x1="17.95" y1="17.95" x2="19.78" y2="19.78"/>
        <line x1="4.22"  y1="19.78" x2="6.05"  y2="17.95"/>
        <line x1="17.95" y1="6.05"  x2="19.78" y2="4.22"/>
      </g>
    </template>

    <!-- ── Partly cloudy ─────────────────────────────────────── -->
    <!-- Small sun upper-right, white cloud overlapping          -->
    <template v-else-if="type === 'partly-cloudy'">
      <!-- Sun disc + 4 diagonal rays (less crowded than 8)      -->
      <circle cx="15.5" cy="7.5" r="3.5" fill="#FFCC00"/>
      <g stroke="#FFCC00" stroke-width="2" stroke-linecap="round">
        <line x1="15.5" y1="2.5"  x2="15.5" y2="4"/>
        <line x1="15.5" y1="11"   x2="15.5" y2="12.5"/>
        <line x1="10.5" y1="7.5"  x2="12"   y2="7.5"/>
        <line x1="19"   y1="7.5"  x2="20.5" y2="7.5"/>
        <line x1="12.56" y1="4.56" x2="13.56" y2="5.56"/>
        <line x1="17.44" y1="9.44" x2="18.44" y2="10.44"/>
        <line x1="12.56" y1="10.44" x2="13.56" y2="9.44"/>
        <line x1="17.44" y1="5.56"  x2="18.44" y2="4.56"/>
      </g>
      <!-- Cloud body (circles) covering lower-left             -->
      <circle cx="7"    cy="16"   r="4.5" fill="#E8F2FC"/>
      <circle cx="11.5" cy="14.5" r="4"   fill="#E8F2FC"/>
      <circle cx="15.5" cy="15.5" r="3"   fill="#E8F2FC"/>
      <!-- fill the flat bottom gap                             -->
      <rect x="2.5" y="16" width="16" height="3.5" rx="1" fill="#E8F2FC"/>
    </template>

    <!-- ── Overcast ──────────────────────────────────────────── -->
    <!-- Layered cloud: darker back + lighter front             -->
    <template v-else-if="type === 'cloudy'">
      <!-- back cloud (darker, shifted up-right)                -->
      <circle cx="15"  cy="11"  r="4"   fill="#A8BCCC"/>
      <circle cx="11"  cy="12"  r="4.5" fill="#A8BCCC"/>
      <rect x="7" y="12" width="12" height="4" rx="1" fill="#A8BCCC"/>
      <!-- front cloud (lighter)                                -->
      <circle cx="8"   cy="14"  r="4.5" fill="#C0D4E4"/>
      <circle cx="12.5" cy="12.5" r="4" fill="#C0D4E4"/>
      <circle cx="16.5" cy="13.5" r="3" fill="#C0D4E4"/>
      <rect x="3.5" y="14" width="16" height="4" rx="1" fill="#C0D4E4"/>
    </template>

    <!-- ── Fog ───────────────────────────────────────────────── -->
    <template v-else-if="type === 'fog'">
      <circle cx="7"  cy="10"  r="4"   fill="#C8D6E8"/>
      <circle cx="12" cy="9"   r="4.5" fill="#C8D6E8"/>
      <circle cx="17" cy="10"  r="3.5" fill="#C8D6E8"/>
      <rect x="3" y="10" width="18" height="4" rx="1" fill="#C8D6E8"/>
      <!-- fog streaks below                                     -->
      <g stroke="#9BAFC4" stroke-width="2.5" stroke-linecap="round">
        <line x1="3"  y1="17"   x2="21" y2="17"/>
        <line x1="5"  y1="20.5" x2="19" y2="20.5"/>
      </g>
    </template>

    <!-- ── Drizzle ───────────────────────────────────────────── -->
    <template v-else-if="type === 'drizzle'">
      <circle cx="7"  cy="10"  r="4"   fill="#C8D6E8"/>
      <circle cx="12" cy="9"   r="4.5" fill="#C8D6E8"/>
      <circle cx="17" cy="10"  r="3.5" fill="#C8D6E8"/>
      <rect x="3" y="10" width="18" height="4" rx="1" fill="#C8D6E8"/>
      <!-- light short drops, slight angle                      -->
      <g stroke="#74AEDE" stroke-width="2" stroke-linecap="round">
        <line x1="8"    y1="18" x2="7.5"  y2="20.5"/>
        <line x1="12.5" y1="18" x2="12"   y2="20.5"/>
        <line x1="17"   y1="18" x2="16.5" y2="20.5"/>
      </g>
    </template>

    <!-- ── Rain ──────────────────────────────────────────────── -->
    <template v-else-if="type === 'rain'">
      <!-- slightly darker cloud for rain                       -->
      <circle cx="7"  cy="10"  r="4"   fill="#9AAFC4"/>
      <circle cx="12" cy="9"   r="4.5" fill="#9AAFC4"/>
      <circle cx="17" cy="10"  r="3.5" fill="#9AAFC4"/>
      <rect x="3" y="10" width="18" height="4" rx="1" fill="#9AAFC4"/>
      <!-- heavy angled drops                                   -->
      <g stroke="#2E85CC" stroke-width="2.5" stroke-linecap="round">
        <line x1="8"    y1="18" x2="6.5"  y2="22"/>
        <line x1="12.5" y1="18" x2="11"   y2="22"/>
        <line x1="17"   y1="18" x2="15.5" y2="22"/>
      </g>
    </template>

    <!-- ── Snow ──────────────────────────────────────────────── -->
    <template v-else-if="type === 'snow'">
      <circle cx="7"  cy="10"  r="4"   fill="#C8D6E8"/>
      <circle cx="12" cy="9"   r="4.5" fill="#C8D6E8"/>
      <circle cx="17" cy="10"  r="3.5" fill="#C8D6E8"/>
      <rect x="3" y="10" width="18" height="4" rx="1" fill="#C8D6E8"/>
      <!-- two snowflake asterisks                              -->
      <g stroke="#7EC8F0" stroke-width="2" stroke-linecap="round">
        <!-- left flake  -->
        <line x1="8"    y1="18"   x2="8"    y2="22"/>
        <line x1="6.27" y1="18.9" x2="9.73" y2="21.1"/>
        <line x1="9.73" y1="18.9" x2="6.27" y2="21.1"/>
        <!-- right flake -->
        <line x1="16"    y1="18"   x2="16"    y2="22"/>
        <line x1="14.27" y1="18.9" x2="17.73" y2="21.1"/>
        <line x1="17.73" y1="18.9" x2="14.27" y2="21.1"/>
      </g>
    </template>

    <!-- ── Thunderstorm ──────────────────────────────────────── -->
    <template v-else-if="type === 'storm'">
      <!-- dark cloud                                           -->
      <circle cx="7"  cy="9"  r="4"   fill="#7A8FA8"/>
      <circle cx="12" cy="8"  r="4.5" fill="#7A8FA8"/>
      <circle cx="17" cy="9"  r="3.5" fill="#7A8FA8"/>
      <rect x="3" y="9" width="18" height="4" rx="1" fill="#7A8FA8"/>
      <!-- bold lightning bolt                                  -->
      <path d="M13.5 14l-4 6h4.5l-4 4" stroke="#FFD700" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>
    </template>

    <!-- ── Unknown ───────────────────────────────────────────── -->
    <template v-else>
      <circle cx="12" cy="12" r="9" fill="#C0CDD8"/>
      <path d="M9.5 9.5a2.5 2.5 0 0 1 5 0c0 2-2.5 2-2.5 4"
            stroke="#5A6A7A" stroke-width="2" stroke-linecap="round"/>
      <line x1="12" y1="17" x2="12.01" y2="17"
            stroke="#5A6A7A" stroke-width="2.5" stroke-linecap="round"/>
    </template>

  </svg>
</template>

<style scoped>
.wi {
  width: 20px;
  height: 20px;
  display: block;
  flex-shrink: 0;
  filter: drop-shadow(0 1px 4px rgba(0, 0, 0, 0.22));
}
</style>
