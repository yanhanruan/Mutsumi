<script setup lang="ts">
/**
 * AboutWindow — small frameless information window opened from the tray.
 */
import { watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { detectLocale, setLocale, useI18n } from '../i18n'
import { useAppConfig } from '../composables/useAppConfig'

const { t } = useI18n()
const { config } = useAppConfig()
const win = getCurrentWindow()

const yohoUrl = 'https://github.com/yanhanruan'
const mutsumiHeadUrl = 'https://github.com/qichengwang408-lab'
const releasesUrl = 'https://github.com/yanhanruan/Mutsumi/releases'
const sourceUrl = 'https://github.com/yanhanruan/Mutsumi'
const appVersion = 'v1.4.0'

watch(
  () => config.value.language,
  lang => setLocale(lang ?? detectLocale()),
  { immediate: true },
)

function closeWindow() { win.close() }
</script>

<template>
  <div class="shell">
    <div class="orb orb-a" />
    <div class="orb orb-b" />
    <div class="orb orb-c" />

    <header class="titlebar" data-tauri-drag-region>
      <div class="title-identity" data-tauri-drag-region>
        <span class="title-logo">🥒</span>
        <span class="title-name">Mutsumi</span>
        <span class="title-sep">·</span>
        <span class="title-sub">{{ t.aboutTitle }}</span>
      </div>

      <div class="win-controls" @mousedown.stop>
        <button class="wbtn wbtn-close" :title="t.close" @click="closeWindow">
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <line x1="1" y1="1" x2="9" y2="9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
            <line x1="9" y1="1" x2="1" y2="9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
          </svg>
        </button>
      </div>
    </header>

    <main class="content">
      <section class="hero">
        <div class="app-mark">🥒</div>
        <div>
          <h1>{{ t.aboutTitle }}</h1>
          <p>{{ t.aboutAppSummaryBody }}</p>
        </div>
      </section>

      <section class="card">
        <h2 class="card-title">{{ t.aboutVersionInfo }}</h2>
        <p class="value"><span class="strong">{{ appVersion }}</span></p>
      </section>

      <section class="card">
        <h2 class="card-title">{{ t.aboutAppSummary }}</h2>
        <p class="value">{{ t.aboutAppSummaryBody }}</p>
        <p class="link-line">
          {{ t.aboutLatestReleaseLead }}
          <a :href="releasesUrl" target="_blank" rel="noreferrer">{{ t.aboutLatestReleaseLink }}</a>
        </p>
        <p class="link-line">
          {{ t.aboutSourceCodeLead }}
          <a :href="sourceUrl" target="_blank" rel="noreferrer">{{ t.aboutSourceCodeLink }}</a>
        </p>
      </section>

      <section class="card">
        <h2 class="card-title">{{ t.aboutMainFeatures }}</h2>
        <ul class="feature-list">
          <li v-for="feature in t.aboutFeaturesList" :key="feature">{{ feature }}</li>
        </ul>
      </section>

      <section class="card">
        <h2 class="card-title">{{ t.aboutDeveloper }}</h2>
        <div class="dev-role-row">
          <span class="dev-role-label">{{ t.aboutDevRoleDev }}</span>
          <span class="dev-role-names">
            <a :href="yohoUrl" target="_blank" rel="noreferrer">{{ t.aboutDeveloperYoho }}</a>
            <a :href="mutsumiHeadUrl" target="_blank" rel="noreferrer">{{ t.aboutDeveloperMutsumiHead }}</a>
          </span>
        </div>
        <div class="dev-role-row">
          <span class="dev-role-label">{{ t.aboutDevRolePromo }}</span>
          <span class="dev-role-names">{{ t.aboutDevPromo }}</span>
        </div>
      </section>

      <section class="card">
        <h2 class="card-title">{{ t.aboutAcknowledgements }}</h2>
        <ul class="feature-list">
          <li v-for="item in t.aboutAckList" :key="item">{{ item }}</li>
        </ul>
      </section>

      <section class="card">
        <h2 class="card-title">{{ t.aboutCopyright }}</h2>
        <p class="value">{{ t.aboutCopyrightMit }}</p>
      </section>
    </main>
  </div>
</template>

<style scoped>
.shell {
  position: relative;
  width: 100%;
  height: 100vh;
  overflow: hidden;
  background: rgba(218, 238, 218, 0.78);
  backdrop-filter: blur(44px) saturate(190%) brightness(1.04);
  -webkit-backdrop-filter: blur(44px) saturate(190%) brightness(1.04);
  border-radius: 12px;
  font-family: system-ui, "Segoe UI", "Noto Sans SC", "Noto Sans JP", sans-serif;
  font-size: 13px;
  color: #1a2e1a;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
}

.orb {
  position: absolute;
  border-radius: 50%;
  pointer-events: none;
  z-index: 0;
}

.orb-a {
  width: 260px;
  height: 260px;
  background: rgba(119, 153, 119, 0.30);
  filter: blur(72px);
  top: -90px;
  right: -80px;
}

.orb-b {
  width: 200px;
  height: 200px;
  background: rgba(80, 160, 130, 0.22);
  filter: blur(66px);
  bottom: -60px;
  left: -55px;
}

.orb-c {
  width: 140px;
  height: 140px;
  background: rgba(155, 200, 110, 0.18);
  filter: blur(54px);
  top: 44%;
  left: 48%;
  transform: translate(-50%, -50%);
}

.titlebar {
  position: relative;
  z-index: 10;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 40px;
  padding: 0 10px 0 14px;
  background: rgba(180, 220, 180, 0.28);
  border-bottom: 1px solid rgba(119, 153, 119, 0.22);
  box-shadow: inset 0 -1px 0 rgba(255, 255, 255, 0.18);
  user-select: none;
  -webkit-user-select: none;
  cursor: default;
}

.title-identity {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 1;
  min-width: 0;
}

.title-logo {
  font-size: 16px;
  line-height: 1;
}

.title-name {
  font-size: 13px;
  font-weight: 700;
  color: #1a3a1a;
}

.title-sep {
  font-size: 11px;
  color: rgba(40, 80, 40, 0.35);
}

.title-sub {
  font-size: 10px;
  font-weight: 500;
  color: rgba(40, 80, 40, 0.50);
  text-transform: uppercase;
  letter-spacing: 0.6px;
}

.win-controls {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.wbtn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 22px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  background: rgba(0, 0, 0, 0.06);
  color: rgba(40, 70, 40, 0.55);
  transition: background 100ms ease, color 100ms ease, transform 80ms ease;
}

.wbtn:hover {
  background: rgba(220, 60, 60, 0.82);
  color: white;
}

.wbtn:active {
  transform: scale(0.90);
}

.content {
  position: relative;
  z-index: 1;
  flex: 1;
  overflow-y: auto;
  padding: 12px 14px 14px;
  display: flex;
  flex-direction: column;
  gap: 9px;
  scrollbar-width: thin;
  scrollbar-color: rgba(119, 153, 119, 0.30) transparent;
}

.content::-webkit-scrollbar {
  width: 4px;
}

.content::-webkit-scrollbar-track {
  background: transparent;
}

.content::-webkit-scrollbar-thumb {
  background: rgba(119, 153, 119, 0.30);
  border-radius: 2px;
}

.hero,
.card {
  background: rgba(255, 255, 255, 0.42);
  backdrop-filter: blur(20px) saturate(170%);
  -webkit-backdrop-filter: blur(20px) saturate(170%);
  border: 1px solid rgba(145, 190, 145, 0.35);
  border-radius: 12px;
  box-shadow:
    0 1px 8px rgba(50, 90, 50, 0.07),
    inset 0 1px 0 rgba(255, 255, 255, 0.80);
}

.hero {
  display: grid;
  grid-template-columns: 48px 1fr;
  gap: 12px;
  align-items: center;
  padding: 13px 13px 12px;
}

.app-mark {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 46px;
  height: 46px;
  border-radius: 12px;
  background: linear-gradient(135deg, rgba(119, 153, 119, 0.22), rgba(255, 255, 255, 0.46));
  border: 1px solid rgba(119, 153, 119, 0.28);
  font-size: 25px;
}

.hero h1 {
  margin: 0 0 5px;
  font-size: 18px;
  line-height: 1.18;
  color: #1a3a1a;
}

.hero p {
  margin: 0;
  font-size: 12px;
  line-height: 1.55;
  color: rgba(38, 76, 38, 0.70);
}

.card {
  padding: 11px 12px 10px;
}

.card-title {
  margin: 0 0 8px;
  font-size: 9.5px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.7px;
  color: rgba(45, 85, 45, 0.55);
}

.value,
.link-line {
  margin: 0;
  font-size: 12.5px;
  line-height: 1.55;
  color: #2c4e2c;
}

.strong {
  font-size: 16px;
  font-weight: 750;
  color: #1a3a1a;
}

.empty {
  min-height: 18px;
}

.dev-role-row {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-top: 6px;
  font-size: 12.5px;
  line-height: 1.55;
}

.dev-role-label {
  flex-shrink: 0;
  font-size: 11px;
  font-weight: 600;
  color: rgba(45, 85, 45, 0.55);
  min-width: 44px;
}

.dev-role-names {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  color: #2c4e2c;
}

.link-line {
  margin-top: 7px;
}

.feature-list {
  margin: 0;
  padding: 0;
  list-style: none;
  display: grid;
  gap: 6px;
}

.feature-list li {
  position: relative;
  padding-left: 15px;
  font-size: 12.5px;
  line-height: 1.45;
  color: #2c4e2c;
}

.feature-list li::before {
  content: '';
  position: absolute;
  left: 1px;
  top: 0.66em;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #779977;
  box-shadow: 0 0 0 3px rgba(119, 153, 119, 0.13);
  transform: translateY(-50%);
}

a {
  color: #315f47;
  font-weight: 700;
  text-decoration: none;
  border-bottom: 1px solid rgba(49, 95, 71, 0.25);
  transition: color 120ms ease, border-color 120ms ease;
}

a:hover {
  color: #1f4934;
  border-color: rgba(49, 95, 71, 0.55);
}
</style>
