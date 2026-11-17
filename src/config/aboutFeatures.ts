import type { PlatformCapabilities } from '../composables/usePlatformCapabilities'
import type { Translations } from '../i18n/types'

/**
 * Build the About feature list from the backend's runtime capability contract.
 *
 * Platform-specific claims stay hidden until the contract resolves. This is
 * deliberately conservative: an About page may omit a feature during an IPC
 * failure, but it must never advertise a control the current platform cannot
 * provide.
 */
export function buildAboutFeatures(
  t: Translations,
  capabilities: PlatformCapabilities | null,
): string[] {
  const features = [t.aboutCompanionFeature]
  const isUsable = (status: PlatformCapabilities[keyof PlatformCapabilities]) =>
    status === 'available' || status === 'degraded'

  if (capabilities && isUsable(capabilities.audioActivity)) {
    features.push(
      capabilities.audioActivity === 'degraded'
        ? t.aboutAudioActivityDegradedFeature
        : t.aboutAudioActivityFeature,
    )
  }

  if (capabilities) {
    const fullMediaController = capabilities.mediaMetadata === 'available'
      && capabilities.mediaTransport === 'available'
      && capabilities.systemVolume === 'available'
    const partialMediaPanel = [capabilities.mediaMetadata, capabilities.mediaTransport]
      .some(isUsable)

    if (fullMediaController) {
      features.push(t.aboutMediaControllerFeature)
    } else if (partialMediaPanel) {
      // This mirrors `musicPanelAvailable`: volume by itself does not create a
      // now-playing panel, while partial metadata/transport must not be
      // advertised as the complete SMTC controller.
      features.push(t.aboutMediaControllerDegradedFeature)
    }
  }

  features.push(...t.aboutFeaturesList)
  return features
}
