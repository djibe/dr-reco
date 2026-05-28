import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification'

/**
 * Send a Windows notification center notification.
 * Requests permission on first call if not already granted.
 *
 * @param {string} title   - Notification title
 * @param {string} body    - Notification body text
 */
export async function notify(title, body) {
  try {
    let granted = await isPermissionGranted()
    if (!granted) {
      const permission = await requestPermission()
      granted = permission === 'granted'
    }
    if (granted) {
      sendNotification({ title, body })
    }
  } catch (e) {
    // Notifications are best-effort — never block the UI on failure
    console.warn('Notification failed:', e)
  }
}
