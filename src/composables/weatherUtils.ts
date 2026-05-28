/**
 * Shared weather classification logic.
 * Kept in a plain .ts module so it can be imported by both
 * WeatherIcon.vue (rendering) and any future consumers.
 */

export type WeatherType =
  | 'clear'
  | 'partly-cloudy'
  | 'cloudy'
  | 'fog'
  | 'drizzle'
  | 'rain'
  | 'snow'
  | 'storm'
  | 'unknown'

/** Maps a WMO weather interpretation code to an icon type. */
export function classify(code: number): WeatherType {
  if (code === 0)                                return 'clear'
  if (code === 1 || code === 2)                  return 'partly-cloudy'
  if (code === 3)                                return 'cloudy'
  if (code === 45 || code === 48)                return 'fog'
  if (code >= 51 && code <= 57)                  return 'drizzle'
  if (code >= 61 && code <= 67)                  return 'rain'
  if (code >= 71 && code <= 77)                  return 'snow'
  if (code >= 80 && code <= 82)                  return 'rain'
  if (code === 85 || code === 86)                return 'snow'
  if (code === 95 || code === 96 || code === 99) return 'storm'
  return 'unknown'
}
