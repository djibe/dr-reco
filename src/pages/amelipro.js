import { invoke } from '@tauri-apps/api/core'

export function renderAmelipro(container, navigate) {
  container.innerHTML = `
    <fluent-button appearance="subtle" id="back-btn">← Accueil</fluent-button>

    <div class="page-header" style="margin-top:12px">
      <h2>🏥 Outils Amelipro</h2>
      <p>Vérification des prérequis logiciels pour Amelipro</p>
    </div>

    <fluent-button appearance="primary" id="launch-btn">▶ Lancer</fluent-button>

    <div class="checks-list" id="checks-list"></div>
    <div id="footer-area" style="margin-top:16px"></div>
  `

  container.querySelector('#back-btn').onclick = () => navigate('welcome')

  const launchBtn  = container.querySelector('#launch-btn')
  const checksList = container.querySelector('#checks-list')
  const footer     = container.querySelector('#footer-area')

  launchBtn.addEventListener('click', async () => {
    launchBtn.disabled = true
    launchBtn.innerHTML = '<fluent-spinner size="tiny" style="margin-right:8px"></fluent-spinner> Vérification en cours…'
    checksList.innerHTML = ''
    footer.innerHTML = ''

    // ── Cryptolib CPS ────────────────────────────────────────────────────────
    const item = addCheck(checksList, 'Cryptolib CPS', 'Lecture du registre Windows…')
    try {
      const r = await invoke('check_cryptolib_version')
      const color = r.is_ok ? 'success' : (r.not_found ? 'danger' : 'warning')
      const icon  = r.is_ok ? '✅' : (r.not_found ? '❌' : '⚠️')
      const badge = r.is_ok
        ? { text: 'OK',              color: 'success' }
        : r.not_found
          ? { text: 'Non installé',        color: 'danger'  }
          : { text: 'Mise à jour requise', color: 'warning' }
      setCheck(item, color, icon, 'Cryptolib CPS', r.detail, badge)
    } catch (e) {
      setCheck(item, 'error', '❌', 'Cryptolib CPS', 'Erreur : ' + e, { text: 'Erreur', color: 'danger' })
    }

    // ── Done ─────────────────────────────────────────────────────────────────
    launchBtn.disabled = false
    launchBtn.innerHTML = '🔄 Relancer'

    const homeBtn = document.createElement('fluent-button')
    homeBtn.setAttribute('appearance', 'secondary')
    homeBtn.innerHTML = '🏠 Retour à l\'accueil'
    homeBtn.onclick = () => navigate('welcome')
    footer.appendChild(homeBtn)
  })
}

function addCheck(list, label, detail) {
  const item = document.createElement('div')
  item.className = 'check-item status-running fade-up'
  item.innerHTML = `
    <div class="check-icon"><fluent-spinner size="tiny"></fluent-spinner></div>
    <div class="check-body">
      <div class="check-label">${label}</div>
      <div class="check-detail">${detail}</div>
    </div>`
  list.appendChild(item)
  return item
}

function setCheck(item, status, icon, label, detail, badge) {
  const badgeHtml = badge
    ? `<fluent-badge appearance="filled" color="${badge.color}">${badge.text}</fluent-badge>`
    : ''
  item.className = `check-item status-${status}`
  item.innerHTML = `
    <div class="check-icon">${icon}</div>
    <div class="check-body">
      <div class="check-label">${label} ${badgeHtml}</div>
      <div class="check-detail">${detail}</div>
    </div>`
}
