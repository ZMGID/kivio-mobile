import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { expect, it } from 'vitest'
import { resolveAppMode } from './appMode'
import { resolveMobileChatView } from './chat/chatRoutes'
import { extensionSubItems } from './chat/Sidebar'
import { EXCLUDED_EXTENSION_IDS, selectShippedExtensions } from './extensions/runtime'
import { settingsNavigation } from './settings/settingsNavigation'

it('opens the chat shell for desktop hashes', () => {
  expect(resolveAppMode('', '')).toBe('chat')
  expect(resolveAppMode('#lens', '')).toBe('chat')
  expect(resolveAppMode('#', '?mode=translate')).toBe('chat')
  expect(resolveAppMode('#chat-popout/...', '')).toBe('chat')
  expect(resolveAppMode('#chat/popout/conv', '')).toBe('chat')
  expect(resolveAppMode('#translator', '')).toBe('chat')
})

it('maps cut chat routes to a conversation and keeps schedules', () => {
  expect(resolveMobileChatView('chat/knowledge')).toBe('conversation')
  expect(resolveMobileChatView('chat/notes')).toBe('conversation')
  expect(resolveMobileChatView('chat/automations')).toBe('conversation')
  expect(resolveMobileChatView('chat/reading')).toBe('conversation')
  expect(resolveMobileChatView('chat/onboarding')).toBe('conversation')
  expect(resolveMobileChatView('chat/schedules')).toBe('schedules')
  expect(resolveMobileChatView('chat')).toBe('conversation')
})

it('ships the mobile settings navigation', () => {
  const cut = ['hotkeys', 'translate', 'lens', 'externalAgents', 'computerControl']
  for (const id of cut) expect(settingsNavigation).not.toContain(id)
  for (const id of ['general', 'themes', 'providers', 'chat', 'memory', 'mixer', 'hooks', 'webSearch', 'sessions', 'usage', 'backup', 'about']) {
    expect(settingsNavigation).toContain(id)
  }
})

it('ships assistants and media without knowledge or notes', () => {
  const ids = extensionSubItems.map(item => item.id)
  expect(ids).not.toContain('knowledge')
  expect(ids).not.toContain('notes')
  expect(ids).toContain('assistants')
  expect(ids).toContain('media')
})

it('does not open the desktop setup wizard from chat or settings', () => {
  const chat = readFileSync(resolve('src/chat/Chat.tsx'), 'utf8')
  const settings = readFileSync(resolve('src/settings/SettingsShell.tsx'), 'utf8')
  expect(chat).not.toContain('syncOnboardingRoute')
  expect(settings).not.toContain('onboardingRestart')
  expect(settings).not.toContain('#chat/onboarding')
})

it('excludes the study extension from the shipped catalog', () => {
  expect(EXCLUDED_EXTENSION_IDS).toContain('study')
  const study = { manifest: { id: 'study' } }
  const other = { manifest: { id: 'other' } }
  expect(selectShippedExtensions([study, other])).toEqual([other])
})
