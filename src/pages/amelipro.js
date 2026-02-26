import { invoke } from '@tauri-apps/api/core'

export function renderAmelipro(container, navigate) {
  container.innerHTML = `
    <button class="back-btn" id="back-btn">← Accueil</button>
    <div class="page-header">
      <h2>🏥 Outils Amelipro</h2>
      <p>Vérification des prérequis logiciels pour Amelipro</p>
    </div>
    <button class="btn-launch" id="launch-btn">
      <span>▶</span> Lancer
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

    // ── Cryptolib CPS version ────────────────────────────────
    const cryptoItem = addCheckItem(checksList, {
      icon: '⏳',
      label: 'Cryptolib CPS',
      detail: 'Lecture du registre Windows…',
      status: 'running',
    })

    try {
      const result = await invoke('check_cryptolib_version')
      setCheckItem(cryptoItem, {
        icon: result.is_ok ? '✅' : (result.not_found ? '❌' : '⚠️'),
        label: 'Cryptolib CPS',
        detail: result.detail,
        badge: result.is_ok
          ? { text: 'OK', cls: 'badge-success' }
          : result.not_found
            ? { text: 'Non installé', cls: 'badge-error' }
            : { text: 'Mise à jour requise', cls: 'badge-warning' },
        status: result.is_ok ? 'success' : (result.not_found ? 'error' : 'warning'),
      })
    } catch (e) {
      setCheckItem(cryptoItem, {
        icon: '❌',
        label: 'Cryptolib CPS',
        detail: 'Erreur lors de la lecture du registre : ' + e,
        badge: { text: 'Erreur', cls: 'badge-error' },
        status: 'error',
      })
    }

    launchBtn.disabled = false
    launchBtn.innerHTML = '<span>🔄</span> Relancer'
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
