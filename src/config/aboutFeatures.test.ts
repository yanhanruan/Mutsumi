import { describe, expect, it } from 'vitest'

import type { PlatformCapabilities } from '../composables/usePlatformCapabilities'
import { en } from '../i18n/locales/en'
import { buildAboutFeatures } from './aboutFeatures'

const windowsCapabilities: PlatformCapabilities = {
  audioActivity: 'available',
  mediaMetadata: 'available',
  mediaTransport: 'available',
  systemVolume: 'available',
  idleDetection: 'available',
  globalCursor: 'available',
  atomicWindowGeometry: 'available',
  deepWebSearch: 'available',
  hardwareDetails: 'available',
  revealInFolder: 'available',
}

const macCapabilities: PlatformCapabilities = {
  ...windowsCapabilities,
  audioActivity: 'degraded',
  mediaMetadata: 'unavailable',
  mediaTransport: 'unavailable',
  systemVolume: 'unavailable',
}

describe('buildAboutFeatures', () => {
  it('advertises the complete Windows audio and media feature set', () => {
    const features = buildAboutFeatures(en, windowsCapabilities)

    expect(features).toContain(en.aboutAudioActivityFeature)
    expect(features).toContain(en.aboutMediaControllerFeature)
    expect(features).not.toContain(en.aboutAudioActivityDegradedFeature)
  })

  it('describes degraded macOS audio without advertising media controls', () => {
    const features = buildAboutFeatures(en, macCapabilities)

    expect(features).toContain(en.aboutAudioActivityDegradedFeature)
    expect(features).not.toContain(en.aboutAudioActivityFeature)
    expect(features).not.toContain(en.aboutMediaControllerFeature)
  })

  it('keeps only cross-platform claims while capability IPC is unresolved', () => {
    const features = buildAboutFeatures(en, null)

    expect(features).toEqual([en.aboutCompanionFeature, ...en.aboutFeaturesList])
  })

  it('does not advertise permission-gated integrations before they are usable', () => {
    const features = buildAboutFeatures(en, {
      ...windowsCapabilities,
      audioActivity: 'permissionRequired',
      mediaMetadata: 'permissionRequired',
      mediaTransport: 'unavailable',
      systemVolume: 'unavailable',
    })

    expect(features).not.toContain(en.aboutAudioActivityFeature)
    expect(features).not.toContain(en.aboutMediaControllerFeature)
  })

  it.each([
    ['metadata only', { mediaMetadata: 'available', mediaTransport: 'unavailable', systemVolume: 'unavailable' }],
    ['transport only', { mediaMetadata: 'unavailable', mediaTransport: 'available', systemVolume: 'unavailable' }],
    ['degraded metadata', { mediaMetadata: 'degraded', mediaTransport: 'unavailable', systemVolume: 'unavailable' }],
    ['no system volume', { mediaMetadata: 'available', mediaTransport: 'available', systemVolume: 'unavailable' }],
  ] as const)('describes %s as partial media integration', (_name, media) => {
    const features = buildAboutFeatures(en, { ...windowsCapabilities, ...media })

    expect(features).toContain(en.aboutMediaControllerDegradedFeature)
    expect(features).not.toContain(en.aboutMediaControllerFeature)
  })

  it('does not turn standalone system volume into a media-panel claim', () => {
    const features = buildAboutFeatures(en, {
      ...windowsCapabilities,
      mediaMetadata: 'unavailable',
      mediaTransport: 'unavailable',
      systemVolume: 'available',
    })

    expect(features).not.toContain(en.aboutMediaControllerFeature)
    expect(features).not.toContain(en.aboutMediaControllerDegradedFeature)
  })
})
