export type DesktopPlatform = 'macos' | 'windows' | 'other'

function navigatorValue(value: string | undefined, fallback: 'userAgent' | 'platform'): string {
  if (value !== undefined) return value
  if (typeof navigator === 'undefined') return ''
  return String(navigator[fallback] ?? '')
}

/**
 * Detect only the desktop platform distinctions used by the Vue chrome.
 * Both inputs are injectable so the decision remains deterministic in tests.
 */
export function detectDesktopPlatform(
  userAgent?: string,
  platform?: string,
): DesktopPlatform {
  const ua = navigatorValue(userAgent, 'userAgent')
  const navPlatform = navigatorValue(platform, 'platform')

  if (/Macintosh/i.test(ua) || /^Mac/i.test(navPlatform)) return 'macos'
  if (/Windows/i.test(ua) || /^Win/i.test(navPlatform)) return 'windows'
  return 'other'
}

export function isMacOSDesktop(userAgent?: string, platform?: string): boolean {
  return detectDesktopPlatform(userAgent, platform) === 'macos'
}
