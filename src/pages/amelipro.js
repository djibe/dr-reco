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
    let cryptolibOutdated = false
    try {
      const r = await invoke('check_cryptolib_version')
      cryptolibOutdated = !r.is_ok && !r.not_found
      const color = r.is_ok ? 'success' : (r.not_found ? 'danger' : 'warning')
      const icon  = r.is_ok ? '✅' : (r.not_found ? '❌' : '⚠️')
      const badge = r.is_ok
        ? { text: 'OK',                  color: 'success' }
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

    if (cryptolibOutdated) addCryptolibDownloadBlock(footer)

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

// ── Cryptolib download block ──────────────────────────────────────────────────
const CRYPTOLIB_URL = 'https://esante.gouv.fr/sites/default/files/media/document/CryptolibCPS-5.2.6_x64.msi'

function addCryptolibDownloadBlock(area) {
  const block = document.createElement('div')
  block.className = 'repair-block fade-up'
  block.innerHTML = `
    <div class="repair-info">
      <span class="repair-icon">📦</span>
      <div>
        <div class="repair-label">Mettre à jour Cryptolib CPS</div>
        <div class="repair-detail">CryptolibCPS-5.2.6_x64.msi — esante.gouv.fr</div>
      </div>
    </div>
    <fluent-button appearance="primary" class="dl-btn">⬇️ Télécharger Cryptolib CPS 5.2.6</fluent-button>
    <div class="dl-result hidden"></div>
  `
  area.insertBefore(block, area.firstChild)

  const btn    = block.querySelector('.dl-btn')
  const result = block.querySelector('.dl-result')

  btn.addEventListener('click', async () => {
    btn.disabled = true
    btn.innerHTML = '<fluent-spinner size="tiny" style="margin-right:8px"></fluent-spinner> Ouverture du téléchargement…'
    try {
      await invoke('open_url', { url: CRYPTOLIB_URL })
      result.className = 'dl-result check-item status-success fade-up'
      result.innerHTML = `<div class="check-icon">✅</div><div class="check-body"><div class="check-detail">Le téléchargement a été ouvert dans votre navigateur. Installez le fichier .msi une fois téléchargé.</div></div>`
      btn.innerHTML = '✅ Téléchargement ouvert'
    } catch (e) {
      result.className = 'dl-result check-item status-warning fade-up'
      result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">Impossible d'ouvrir le lien : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = '⬇️ Réessayer'
    }
  })
}
