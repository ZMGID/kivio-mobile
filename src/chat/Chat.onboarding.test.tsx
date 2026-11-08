import { render, screen } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { api, type Settings } from '../api/tauri'
import { getSettingsCached, getSettingsSnapshotCached, saveSettingsSnapshotCached } from '../api/settingsCache'
import { makeSettings } from '../settings/tabs/testFixtures'
import { chatApi } from './api'
import Chat from './Chat'

vi.mock('@xterm/xterm', () => ({ Terminal: class {} }))
vi.mock('@xterm/addon-webgl', () => ({ WebglAddon: class {} }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ onFocusChanged: async () => () => {} }) }))
vi.mock('../api/settingsCache', async original => ({
  ...await original<typeof import('../api/settingsCache')>(),
  getSettingsCached: vi.fn(),
  getSettingsSnapshotCached: vi.fn(),
  saveSettingsSnapshotCached: vi.fn(),
}))

afterEach(() => { vi.restoreAllMocks(); window.localStorage.clear() })

it('opens the chat composer instead of the desktop setup wizard', async () => {
  for (const key of Object.keys(api) as Array<keyof typeof api>) {
    if (key.startsWith('on') && typeof api[key] === 'function') vi.spyOn(api, key).mockResolvedValue(() => {})
  }
  vi.spyOn(api, 'chatSyncState').mockResolvedValue(undefined)
  vi.spyOn(api, 'scheduledTasksSnapshot').mockResolvedValue({ tasks: [], liveRuns: [] })
  vi.spyOn(chatApi, 'getConversations').mockResolvedValue([])
  vi.spyOn(chatApi, 'getProjects').mockResolvedValue([])
  vi.spyOn(chatApi, 'getSets').mockResolvedValue([])
  vi.spyOn(chatApi, 'getAssistants').mockResolvedValue([])
  vi.spyOn(chatApi, 'getConversationPins').mockResolvedValue({})
  const initial = makeSettings({
    settingsLanguage: 'zh',
    onboardingStatus: 'pending',
    chat: {} as Settings['chat'],
    chatModel: 'stale-chat-model',
  })
  vi.mocked(getSettingsCached).mockResolvedValue(initial)
  vi.mocked(getSettingsSnapshotCached).mockResolvedValue({ settings: initial, version: { epoch: 'test', revision: 1 } })
  vi.mocked(saveSettingsSnapshotCached).mockResolvedValue({ settings: initial, version: { epoch: 'test', revision: 1 } })
  window.history.replaceState(null, '', '#chat/onboarding')
  render(<Chat onSettingsChange={() => {}} />)
  expect(await screen.findByRole('textbox', { name: '有问题尽管问…' })).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: '开始配置' })).not.toBeInTheDocument()
  expect(screen.queryByText('欢迎使用 Kivio Desktop')).not.toBeInTheDocument()
  expect(screen.queryByText('外部 CLI 代理')).not.toBeInTheDocument()
  expect(screen.queryByText('笔记与知识库')).not.toBeInTheDocument()
  expect(screen.queryByText('Lens 与翻译')).not.toBeInTheDocument()
})
