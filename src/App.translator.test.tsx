import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import App from './App'

vi.mock('./api/tauri', () => ({
  isTauriRuntime: () => false,
  api: { showWindow: vi.fn(), focusWindow: vi.fn() },
}))
vi.mock('./api/settingsCache', () => ({
  getSettingsCached: async () => ({ settingsLanguage: 'zh' }),
  subscribeSettings: () => () => {},
}))
vi.mock('./theme/theme', () => ({ applyThemeSettings: vi.fn(), disposeTheme: vi.fn() }))
vi.mock('./chat/ChatWindowHost', () => ({
  ChatWindowHost: ({ children }: { children?: ReactNode }) => <div data-testid="chat-shell">{children}</div>,
}))
vi.mock('./chat/Chat', () => ({ default: () => <div data-testid="chat-screen" /> }))

describe('mobile app entry', () => {
  it.each(['', '#lens', '#translator', '#chat/popout/conv'])('opens chat instead of a desktop surface for %s', async (hash) => {
    window.location.hash = hash
    render(<App />)
    expect(await screen.findByTestId('chat-shell')).toBeInTheDocument()
    expect(screen.getByTestId('chat-screen')).toBeInTheDocument()
    expect(screen.queryByText('译文会显示在这里')).not.toBeInTheDocument()
  })
})
