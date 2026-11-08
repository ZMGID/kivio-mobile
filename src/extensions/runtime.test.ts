import { beforeEach, expect, it, vi } from 'vitest'

vi.mock('../api/extensions', () => ({ extensionsApi: {
  list: async () => [],
  subscribe: async () => () => {},
  setEnabled: async () => [],
} }))

beforeEach(() => {
  vi.resetModules()
})

it('selectShippedExtensions removes study and keeps any other id', async () => {
  const { selectShippedExtensions } = await import('./runtime')
  const study = { manifest: { id: 'study' } }
  const other = { manifest: { id: 'other' } }
  expect(selectShippedExtensions([study, other])).toEqual([other])
})

it('ships a catalog that does not include study', async () => {
  const { catalog } = await import('./runtime')
  expect(catalog.map(entry => entry.manifest.id)).not.toContain('study')
})

it('rejects enabling study as an unknown extension', async () => {
  const { setExtensionEnabled } = await import('./runtime')
  await expect(setExtensionEnabled('study', true)).rejects.toThrow('未知扩展')
})
