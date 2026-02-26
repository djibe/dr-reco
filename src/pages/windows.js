import { invoke } from '@tauri-apps/api/core'

export function renderWindows(container, navigate) {
  container.innerHTML = `
    <button class="back-btn" id="back-btn">← Accueil</button>
    <div class="page-header">
      <h2>🪟 Windows &amp; Maintenance</h2>
      <p>Vérification de l'état du système, intégrité des fichiers et santé du disque</p>
    </div>
    <button class="btn-launch" id="launch-btn">
      <span>▶</span> Lancer la vérification
    </button>
    <div class="checks-list" id="checks-list"></div>
  `

  container.querySelector('#back-btn').onclick = () => navigate('welcome')

  const launchBtn = container.querySelector('#launch-btn')
  const checksList = container.querySelector('#checks-list')

  launchBtn.addEventListener('click', async () => {
    launchBtn.disabled = true
    launchBtn.innerHTML = '<span class="spinner"></span> Vérification en cours…'
    checksList.innerHTML = ''

    // ── 1. Windows version ───────────────────────────────────
    const versionItem = addCheckItem(checksList, {
      icon: '⏳',
      label: 'Version de Windows',
      detail: 'Récupération de la version…',
      status: 'running',
    })

    try {
      const result = await invoke('check_windows_version')
      const ok = result.is_ok
      const detail = result.detail || ''
      setCheckItem(versionItem, {
        icon: ok ? '✅' : '⚠️',
        label: 'Version de Windows',
        detail: detail,
        badge: ok ? { text: 'OK', cls: 'badge-success' } : { text: 'Mise à jour requise', cls: 'badge-warning' },
        status: ok ? 'success' : 'warning',
      })
    } catch (e) {
      setCheckItem(versionItem, {
        icon: '❌', label: 'Version de Windows',
        detail: 'Impossible de récupérer la version : ' + e,
        badge: { text: 'Erreur', cls: 'badge-error' }, status: 'error',
      })
    }

    // ── 2. SFC /scannow ──────────────────────────────────────
    const sfcItem = addCheckItem(checksList, {
      icon: '⏳',
      label: 'Intégrité des fichiers système (SFC)',
      detail: 'Analyse en cours — cette opération peut prendre plusieurs minutes…',
      status: 'running',
    })

    try {
      const result = await invoke('run_sfc_check')
      setCheckItem(sfcItem, {
        icon: result.is_ok ? '✅' : '🔧',
        label: 'Intégrité des fichiers système (SFC)',
        detail: result.detail,
        badge: result.is_ok
          ? { text: 'OK', cls: 'badge-success' }
          : { text: 'Réparation planifiée', cls: 'badge-warning' },
        status: result.is_ok ? 'success' : 'warning',
      })
    } catch (e) {
      setCheckItem(sfcItem, {
        icon: '❌', label: 'Intégrité des fichiers système (SFC)',
        detail: 'Erreur : ' + e,
        badge: { text: 'Erreur', cls: 'badge-error' }, status: 'error',
      })
    }

    // ── 3. CHKDSK ────────────────────────────────────────────
    const chkItem = addCheckItem(checksList, {
      icon: '⏳',
      label: 'Santé du disque C: (CHKDSK)',
      detail: 'Analyse du disque en cours…',
      status: 'running',
    })

    try {
      const result = await invoke('run_chkdsk')
      setCheckItem(chkItem, {
        icon: result.is_ok ? '✅' : '🔧',
        label: 'Santé du disque C: (CHKDSK)',
        detail: result.detail,
        badge: result.is_ok
          ? { text: 'OK', cls: 'badge-success' }
          : { text: 'Réparation planifiée', cls: 'badge-warning' },
        status: result.is_ok ? 'success' : 'warning',
      })
    } catch (e) {
      setCheckItem(chkItem, {
        icon: '❌', label: 'Santé du disque C: (CHKDSK)',
        detail: 'Erreur : ' + e,
        badge: { text: 'Erreur', cls: 'badge-error' }, status: 'error',
      })
    }

    launchBtn.disabled = false
    launchBtn.innerHTML = '<span>🔄</span> Relancer la vérification'
  })
}

function addCheckItem(list, { icon, label, detail, status }) {
  const item = document.createElement('div')
  item.className = `check-item status-${status} fade-up`
  item.innerHTML = `
    <div class="check-icon">${icon}</div>
    <div class="check-body">
      <div class="check-label">${label}</div>
      <div class="check-detail">${detail}</div>
    </div>
  `
  list.appendChild(item)
  return item
}

function setCheckItem(item, { icon, label, detail, badge, status }) {
  item.className = `check-item status-${status}`
  const badgeHtml = badge ? `<span class="check-badge ${badge.cls}">${badge.text}</span>` : ''
  item.innerHTML = `
    <div class="check-icon">${icon}</div>
    <div class="check-body">
      <div class="check-label">${label}${badgeHtml}</div>
      <div class="check-detail">${detail}</div>
    </div>
  `
}
