export const settingsNavigation = [
  'general',
  'themes',
  'providers',
  'chat',
  'memory',
  'mixer',
  'hooks',
  'sessions',
  'webSearch',
  'usage',
  'backup',
  'about',
] as const

export type MobileSettingsTab = typeof settingsNavigation[number]
