import { describe, expect, it } from 'vitest'

import { detectDesktopPlatform, isMacOSDesktop } from './desktopPlatform'

describe('desktopPlatform', () => {
  it('detects a macOS WKWebView user agent', () => {
    expect(detectDesktopPlatform(
      'Mozilla/5.0 (Macintosh; Intel Mac OS X 13_6) AppleWebKit/605.1.15',
      'MacIntel',
    )).toBe('macos')
  })

  it('falls back to navigator.platform when the user agent is reduced', () => {
    expect(detectDesktopPlatform('Mozilla/5.0 AppleWebKit/605.1.15', 'MacIntel')).toBe('macos')
  })

  it('does not mistake an iPhone for the desktop macOS shell', () => {
    expect(isMacOSDesktop(
      'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)',
      'iPhone',
    )).toBe(false)
  })

  it('keeps Windows and other desktop platforms distinct', () => {
    expect(detectDesktopPlatform('Mozilla/5.0 (Windows NT 10.0; Win64; x64)', 'Win32')).toBe('windows')
    expect(detectDesktopPlatform('Mozilla/5.0 (X11; Linux x86_64)', 'Linux x86_64')).toBe('other')
  })
})
