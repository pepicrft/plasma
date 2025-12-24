import { describe, it, expect, vi, beforeEach } from 'vitest'
import { checkForUpdates } from './updater'

// Mock the Tauri plugins
vi.mock('@tauri-apps/plugin-updater', () => ({
  check: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: vi.fn(),
}))

describe('updater', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    // Clear console spies
    vi.spyOn(console, 'log').mockImplementation(() => {})
    vi.spyOn(console, 'error').mockImplementation(() => {})
  })

  describe('checkForUpdates', () => {
    it('should log when no updates are available', async () => {
      const { check } = await import('@tauri-apps/plugin-updater')
      vi.mocked(check).mockResolvedValue(null)

      await checkForUpdates()

      expect(check).toHaveBeenCalledOnce()
      expect(console.log).toHaveBeenCalledWith('No updates available')
    })

    it('should prompt user when update is available', async () => {
      const { check } = await import('@tauri-apps/plugin-updater')
      const { ask } = await import('@tauri-apps/plugin-dialog')

      const mockUpdate = {
        version: '1.2.3',
        body: 'New features and bug fixes',
        downloadAndInstall: vi.fn().mockResolvedValue(undefined),
      }

      vi.mocked(check).mockResolvedValue(mockUpdate as any)
      vi.mocked(ask).mockResolvedValue(false)

      await checkForUpdates()

      expect(check).toHaveBeenCalledOnce()
      expect(console.log).toHaveBeenCalledWith('Update available: 1.2.3')
      expect(ask).toHaveBeenCalledWith(
        'Update to 1.2.3 is available!\n\nRelease notes: New features and bug fixes',
        {
          title: 'Update Available',
          kind: 'info',
          okLabel: 'Update',
          cancelLabel: 'Later'
        }
      )
      expect(mockUpdate.downloadAndInstall).not.toHaveBeenCalled()
    })

    it('should download and install update when user accepts', async () => {
      const { check } = await import('@tauri-apps/plugin-updater')
      const { ask } = await import('@tauri-apps/plugin-dialog')
      const { relaunch } = await import('@tauri-apps/plugin-process')

      const mockUpdate = {
        version: '1.2.3',
        body: 'New features and bug fixes',
        downloadAndInstall: vi.fn().mockResolvedValue(undefined),
      }

      vi.mocked(check).mockResolvedValue(mockUpdate as any)
      vi.mocked(ask).mockResolvedValue(true)
      vi.mocked(relaunch).mockResolvedValue(undefined)

      await checkForUpdates()

      expect(console.log).toHaveBeenCalledWith('Downloading and installing update...')
      expect(mockUpdate.downloadAndInstall).toHaveBeenCalledOnce()
      expect(relaunch).toHaveBeenCalledOnce()
    })

    it('should handle errors gracefully', async () => {
      const { check } = await import('@tauri-apps/plugin-updater')
      const error = new Error('Network error')

      vi.mocked(check).mockRejectedValue(error)

      await checkForUpdates()

      expect(console.error).toHaveBeenCalledWith('Update check failed:', error)
    })

    it('should handle download errors', async () => {
      const { check } = await import('@tauri-apps/plugin-updater')
      const { ask } = await import('@tauri-apps/plugin-dialog')

      const downloadError = new Error('Download failed')
      const mockUpdate = {
        version: '1.2.3',
        body: 'New features',
        downloadAndInstall: vi.fn().mockRejectedValue(downloadError),
      }

      vi.mocked(check).mockResolvedValue(mockUpdate as any)
      vi.mocked(ask).mockResolvedValue(true)

      await checkForUpdates()

      expect(console.error).toHaveBeenCalledWith('Update check failed:', downloadError)
    })

    it('should not download if user cancels', async () => {
      const { check } = await import('@tauri-apps/plugin-updater')
      const { ask } = await import('@tauri-apps/plugin-dialog')

      const mockUpdate = {
        version: '1.2.3',
        body: 'New features',
        downloadAndInstall: vi.fn(),
      }

      vi.mocked(check).mockResolvedValue(mockUpdate as any)
      vi.mocked(ask).mockResolvedValue(false)

      await checkForUpdates()

      expect(mockUpdate.downloadAndInstall).not.toHaveBeenCalled()
    })
  })
})
