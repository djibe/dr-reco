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
    let extensionMissingBrowser = null
    let detectedBrowser = null
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

    // ── Services CNAM ─────────────────────────────────────────────────────────
    const cnamItem = addCheck(checksList, 'Services CNAM (SrvSvCnam)', 'Lecture du registre Windows…')
    let cnamOutdated = false
    try {
      const r = await invoke('check_services_cnam')
      cnamOutdated = !r.is_ok
      const color = r.is_ok ? 'success' : (r.not_found ? 'danger' : 'warning')
      const icon  = r.is_ok ? '✅' : (r.not_found ? '❌' : '⚠️')
      const badge = r.is_ok
        ? { text: 'Conforme',            color: 'success' }
        : r.not_found
          ? { text: 'Non installé',        color: 'danger'  }
          : { text: 'Mise à jour requise', color: 'warning' }
      setCheck(cnamItem, color, icon, 'Services CNAM (SrvSvCnam)', r.detail, badge)
    } catch (e) {
      setCheck(cnamItem, 'warning', '⚠️', 'Services CNAM (SrvSvCnam)',
        `La vérification n'a pas pu être lancée : ${e}`,
        { text: 'Indisponible', color: 'warning' })
    }

        // ── Lecteur de carte à puce ───────────────────────────────────────────────
    const scItem = addCheck(checksList, 'Lecteur de carte à puce', 'Recherche de périphériques connectés…')
    try {
      const r = await invoke('check_smartcard_reader')
      if (r.ps_unavailable) {
        setCheck(scItem, 'warning', '⚠️', 'Lecteur de carte à puce',
          r.detail, { text: 'PowerShell indisponible', color: 'warning' })
      } else {
        setCheck(scItem, r.is_ok ? 'success' : 'error', r.is_ok ? '✅' : '❌',
          'Lecteur de carte à puce', r.detail,
          r.is_ok ? { text: 'Connecté', color: 'success' } : { text: 'Non détecté', color: 'danger' })
      }
    } catch (e) {
      setCheck(scItem, 'warning', '⚠️', 'Lecteur de carte à puce',
        `La détection n'a pas pu être lancée : ${e}`,
        { text: 'Indisponible', color: 'warning' })
    }

        // ── Navigateur & extension Lecture Carte Vitale ──────────────────────────
    const brItem = addCheck(checksList, 'Navigateur & extension Carte Vitale', 'Détection du navigateur par défaut…')
    try {
      const r = await invoke('check_browser_and_extension')

      if (r.ps_unavailable) {
        setCheck(brItem, 'warning', '⚠️', 'Navigateur & extension Carte Vitale',
          r.detail, { text: 'PowerShell indisponible', color: 'warning' })

      } else if (!r.extension_checked) {
        // Browser detected but not Chrome or Firefox
        detectedBrowser = r.browser
        const icon = r.browser === 'unknown' ? '❌' : '⚠️'
        const status = r.browser === 'unknown' ? 'error' : 'warning'
        const badge = r.browser === 'unknown'
          ? { text: 'Navigateur inconnu', color: 'danger' }
          : { text: r.browser_label, color: 'warning' }
        setCheck(brItem, status, icon, 'Navigateur & extension Carte Vitale',
          r.detail, badge)

      } else if (r.extension_found) {
        detectedBrowser = r.browser
        setCheck(brItem, 'success', '✅', 'Navigateur & extension Carte Vitale',
          r.detail, { text: r.browser_label, color: 'success' })

      } else {
        // Extension missing — store browser for download block
        detectedBrowser = r.browser
        extensionMissingBrowser = r.browser
        setCheck(brItem, 'error', '❌', 'Navigateur & extension Carte Vitale',
          r.detail, { text: 'Extension manquante', color: 'danger' })
      }
    } catch (e) {
      setCheck(brItem, 'warning', '⚠️', 'Navigateur & extension Carte Vitale',
        `La vérification n'a pas pu être lancée : ${e}`,
        { text: 'Indisponible', color: 'warning' })
    }

    // ── Version du navigateur par défaut ──────────────────────────────────────
    let browserOutdated = false
    let browserSlugForUpdate = null
    if (detectedBrowser === 'chrome' || detectedBrowser === 'firefox') {
      const vbrItem = addCheck(checksList, `Version de ${detectedBrowser === 'chrome' ? 'Google Chrome' : 'Mozilla Firefox'}`, 'Lecture de la version installée…')
      try {
        const r = await invoke('check_browser_version', { browser: detectedBrowser })
        if (!r.is_ok && r.installed !== '') {
          browserOutdated = true
          browserSlugForUpdate = detectedBrowser
        }
        const status = r.is_ok ? 'success' : (r.installed === '' ? 'warning' : 'warning')
        const icon   = r.is_ok ? '✅' : '⚠️'
        const badge  = r.is_ok
          ? { text: `v${r.installed}`, color: 'success' }
          : r.installed === ''
            ? { text: 'Non détecté', color: 'warning' }
            : { text: 'Mise à jour requise', color: 'warning' }
        setCheck(vbrItem, status, icon,
          `Version de ${r.browser_label}`, r.detail, badge)
      } catch (e) {
        setCheck(vbrItem, 'warning', '⚠️',
          'Version du navigateur',
          `La vérification de version n'a pas pu être lancée : ${e}`,
          { text: 'Indisponible', color: 'warning' })
      }
    }

        // ── Done ─────────────────────────────────────────────────────────────────
    launchBtn.disabled = false
    launchBtn.innerHTML = '🔄 Relancer'

    if (cryptolibOutdated) addCryptolibDownloadBlock(footer)
    if (cnamOutdated)      addCnamDownloadBlock(footer)
    if (extensionMissingBrowser) addExtensionDownloadBlock(footer, extensionMissingBrowser)
    if (browserOutdated && browserSlugForUpdate) addBrowserUpdateBlock(footer, browserSlugForUpdate)

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

// ── Services CNAM download block ──────────────────────────────────────────────
const CNAM_URL = 'https://espacepro.ameli.fr/inscription/#/aide#blocs-2'

function addCnamDownloadBlock(area) {
  const block = document.createElement('div')
  block.className = 'repair-block fade-up'
  block.innerHTML = `
    <div class="repair-info">
      <span class="repair-icon">📦</span>
      <div>
        <div class="repair-label">Mettre à jour les Services CNAM</div>
        <div class="repair-detail">espacepro.ameli.fr — Section Aide</div>
      </div>
    </div>
    <fluent-button appearance="primary" class="cnam-btn">⬇️ Télécharger les Services CNAM</fluent-button>
    <div class="cnam-result hidden"></div>
  `
  area.insertBefore(block, area.firstChild)

  const btn    = block.querySelector('.cnam-btn')
  const result = block.querySelector('.cnam-result')

  btn.addEventListener('click', async () => {
    btn.disabled = true
    btn.innerHTML = '<fluent-spinner size="tiny" style="margin-right:8px"></fluent-spinner> Ouverture…'
    try {
      await invoke('open_url', { url: CNAM_URL })
      result.className = 'cnam-result check-item status-success fade-up'
      result.innerHTML = `<div class="check-icon">✅</div><div class="check-body"><div class="check-detail">La page de téléchargement AmeliPro a été ouverte dans votre navigateur.</div></div>`
      btn.innerHTML = '✅ Page ouverte'
    } catch (e) {
      result.className = 'cnam-result check-item status-warning fade-up'
      result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">Impossible d'ouvrir le lien : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = '⬇️ Réessayer'
    }
  })
}

// ── Extension download block ──────────────────────────────────────────────────
const EXTENSION_URLS = {
  chrome:  'https://chromewebstore.google.com/detail/lecture-carte-vitale/kpjpglcbcgnblkigbedgaoegjbifejka?hl=fr',
  firefox: 'https://addons.mozilla.org/fr/firefox/addon/lecture-carte-vitale/',
}

function addExtensionDownloadBlock(area, browser) {
  const url         = EXTENSION_URLS[browser]
  const browserName = browser === 'chrome' ? 'Google Chrome' : 'Mozilla Firefox'
  const store       = browser === 'chrome' ? 'Chrome Web Store' : 'Firefox Add-ons'

  const block = document.createElement('div')
  block.className = 'repair-block fade-up'
  block.innerHTML = `
    <div class="repair-info">
      <span class="repair-icon">🧩</span>
      <div>
        <div class="repair-label">Installer l'extension Lecture Carte Vitale</div>
        <div class="repair-detail">${store} — ${browserName}</div>
      </div>
    </div>
    <fluent-button appearance="primary" class="ext-btn">🧩 Installer l'extension</fluent-button>
    <div class="ext-result hidden"></div>
  `
  area.insertBefore(block, area.firstChild)

  const btn    = block.querySelector('.ext-btn')
  const result = block.querySelector('.ext-result')

  btn.addEventListener('click', async () => {
    btn.disabled = true
    btn.innerHTML = '<fluent-spinner size="tiny" style="margin-right:8px"></fluent-spinner> Ouverture…'
    try {
      await invoke('open_url', { url })
      result.className = 'ext-result check-item status-success fade-up'
      result.innerHTML = `<div class="check-icon">✅</div><div class="check-body"><div class="check-detail">La page ${store} a été ouverte dans votre navigateur. Cliquez sur "Ajouter" pour installer l'extension.</div></div>`
      btn.innerHTML = '✅ Page ouverte'
    } catch (e) {
      result.className = 'ext-result check-item status-warning fade-up'
      result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">Impossible d'ouvrir le lien : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = '🧩 Réessayer'
    }
  })
}

// ── Browser update block ──────────────────────────────────────────────────────
function addBrowserUpdateBlock(area, browser) {
  const browserName = browser === 'chrome' ? 'Google Chrome' : 'Mozilla Firefox'

  const block = document.createElement('div')
  block.className = 'repair-block fade-up'
  block.innerHTML = `
    <div class="repair-info">
      <span class="repair-icon">🔄</span>
      <div>
        <div class="repair-label">Mettre à jour ${browserName}</div>
        <div class="repair-detail">Ouvre la page de mise à jour intégrée du navigateur</div>
      </div>
    </div>
    <fluent-button appearance="primary" class="browser-update-btn">🔄 Mettre à jour ${browserName}</fluent-button>
    <div class="browser-update-result hidden"></div>
  `
  area.insertBefore(block, area.firstChild)

  const btn    = block.querySelector('.browser-update-btn')
  const result = block.querySelector('.browser-update-result')

  btn.addEventListener('click', async () => {
    btn.disabled = true
    btn.innerHTML = '<fluent-spinner size="tiny" style="margin-right:8px"></fluent-spinner> Ouverture…'
    try {
      const r = await invoke('launch_browser_update', { browser })
      if (r.ps_unavailable) {
        result.className = 'browser-update-result check-item status-warning fade-up'
        result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">${r.detail}</div></div>`
        btn.disabled = false
        btn.innerHTML = '🔄 Réessayer'
      } else {
        result.className = 'browser-update-result check-item status-success fade-up'
        result.innerHTML = `<div class="check-icon">✅</div><div class="check-body"><div class="check-detail">${r.detail}</div></div>`
        btn.innerHTML = `✅ ${browserName} ouvert`
      }
    } catch (e) {
      result.className = 'browser-update-result check-item status-warning fade-up'
      result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">Impossible de lancer la mise à jour : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = '🔄 Réessayer'
    }
  })
}
