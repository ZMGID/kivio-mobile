export type AppMode = 'chat'

/** Mobile always opens the chat shell. Lens, translator, popout, and empty hash are desktop surfaces. */
export function resolveAppMode(_hash: string, _search: string): AppMode {
  void _hash
  void _search
  return 'chat'
}
