import { invoke } from '@tauri-apps/api/core'
import { notify } from '../notify.js'

export function renderAmelipro(container, navigate) {
  container.innerHTML = `
    <div class="dr-nav-card">
      <button class="btn-dr-subtle mb-3" id="back-btn">← Accueil</button>

      <div class="dr-page-header">
        <svg xmlns="http://www.w3.org/2000/svg" height="24px" viewBox="0 -960 960 960" width="24px" fill="var(--dr-primary)"><path d="M420-360h120l-23-129q20-10 31.5-29t11.5-42q0-33-23.5-56.5T480-640q-33 0-56.5 23.5T400-560q0 23 11.5 42t31.5 29l-23 129Zm60 280q-139-35-229.5-159.5T160-516v-244l320-120 320 120v244q0 152-90.5 276.5T480-80Zm0-84q104-33 172-132t68-220v-189l-240-90-240 90v189q0 121 68 220t172 132Zm0-316Z"/></svg>
        <h2>Outils Amelipro</h2>
      </div>
      <p>Vérification des prérequis logiciels pour Amelipro</p>

      <button class="btn-dr-primary" id="launch-btn" style="width: fit-content">▶ Lancer l’analyse</button>

      <div class="dr-checks" id="checks-list"></div>
      <div class="dr-repairs" id="footer-area" style="margin-top:1.25rem"></div>
    </div>
  `

  container.querySelector('#back-btn').onclick = () => navigate('welcome')

  const launchBtn  = container.querySelector('#launch-btn')
  const checksList = container.querySelector('#checks-list')
  const footer     = container.querySelector('#footer-area')

  launchBtn.addEventListener('click', async () => {
    launchBtn.disabled = true
    launchBtn.innerHTML = '<span class="dr-spinner"></span> Vérification en cours…'
    checksList.innerHTML = ''
    footer.innerHTML = ''

    let cryptolibOutdated      = false
    let usbSuspendActive       = false
    let extensionMissingBrowser = null
    let detectedBrowser        = null
    let browserOutdated        = false
    let browserSlugForUpdate   = null
    let cnamOutdated           = false

    // ── Cryptolib CPS ─────────────────────────────────────────────────────────
    const item = addCheck(checksList, 'Cryptolib CPS', 'Lecture du registre Windows…')
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

    // ── Navigateur & extension Carte Vitale ───────────────────────────────────
    const brItem = addCheck(checksList, 'Navigateur & extension Carte Vitale', 'Détection du navigateur par défaut…')
    try {
      const r = await invoke('check_browser_and_extension')
      if (r.ps_unavailable) {
        setCheck(brItem, 'warning', '⚠️', 'Navigateur & extension Carte Vitale',
          r.detail, { text: 'PowerShell indisponible', color: 'warning' })
      } else if (!r.extension_checked) {
        detectedBrowser = r.browser
        const icon   = r.browser === 'unknown' ? '❌' : '⚠️'
        const status = r.browser === 'unknown' ? 'error' : 'warning'
        const badge  = r.browser === 'unknown'
          ? { text: 'Navigateur inconnu', color: 'danger' }
          : { text: r.browser_label, color: 'warning' }
        setCheck(brItem, status, icon, 'Navigateur & extension Carte Vitale', r.detail, badge)
      } else if (r.extension_found) {
        detectedBrowser = r.browser
        setCheck(brItem, 'success', '✅', 'Navigateur & extension Carte Vitale',
          r.detail, { text: r.browser_label, color: 'success' })
      } else {
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
    if (detectedBrowser === 'chrome' || detectedBrowser === 'firefox' || detectedBrowser === 'edge') {
      const browserDisplayName = detectedBrowser === 'chrome' ? 'Google Chrome'
        : detectedBrowser === 'firefox' ? 'Mozilla Firefox' : 'Microsoft Edge'
      const vbrItem = addCheck(checksList, `Version de ${browserDisplayName}`, 'Lecture de la version installée…')
      try {
        const r = await invoke('check_browser_version', { browser: detectedBrowser })
        if (!r.is_ok && r.installed !== '') {
          browserOutdated = true
          browserSlugForUpdate = detectedBrowser
        }
        const icon  = r.is_ok ? '✅' : '⚠️'
        const badge = r.is_ok
          ? { text: `v${r.installed}`, color: 'success' }
          : r.installed === ''
            ? { text: 'Non détecté', color: 'warning' }
            : { text: 'Mise à jour requise', color: 'warning' }
        setCheck(vbrItem, r.is_ok ? 'success' : 'warning', icon,
          `Version de ${r.browser_label}`, r.detail, badge)
      } catch (e) {
        setCheck(vbrItem, 'warning', '⚠️', 'Version du navigateur',
          `La vérification de version n'a pas pu être lancée : ${e}`,
          { text: 'Indisponible', color: 'warning' })
      }
    }

    // ── Mise en veille sélective USB ──────────────────────────────────────────
    const usbItem = addCheck(checksList, 'Mise en veille sélective USB', 'Lecture du plan d\'alimentation…')
    try {
      const r = await invoke('check_usb_suspend')
      if (r.not_found) {
        usbItem.remove()
      } else if (r.ps_unavailable) {
        setCheck(usbItem, 'warning', '⚠️', 'Mise en veille sélective USB',
          r.detail, { text: 'Indisponible', color: 'warning' })
      } else {
        usbSuspendActive = !r.is_ok
        setCheck(usbItem, r.is_ok ? 'success' : 'warning', r.is_ok ? '✅' : '⚠️',
          'Mise en veille sélective USB', r.detail,
          r.is_ok ? { text: 'Désactivée', color: 'success' } : { text: 'Activée', color: 'warning' })
      }
    } catch (e) {
      setCheck(usbItem, 'warning', '⚠️', 'Mise en veille sélective USB',
        `La vérification n'a pas pu être lancée : ${e}`,
        { text: 'Indisponible', color: 'warning' })
    }

    // ── Done ──────────────────────────────────────────────────────────────────
    launchBtn.disabled = false
    launchBtn.innerHTML = '🔄 Relancer l’analyse'

    const issueCount = [cryptolibOutdated, cnamOutdated, extensionMissingBrowser, browserOutdated, usbSuspendActive].filter(Boolean).length
    if (issueCount === 0) {
      notify('Dr Reco — AmeliPro', '✅ Vérification terminée — Aucun problème détecté.')
    } else {
      notify('Dr Reco — AmeliPro', `⚠️ Vérification terminée — ${issueCount} problème${issueCount > 1 ? 's' : ''} détecté${issueCount > 1 ? 's' : ''}.`)
    }

    if (cryptolibOutdated)                     addDownloadBlock(footer, { icon: '📦', label: 'Mettre à jour Cryptolib CPS',          cmd: 'CryptolibCPS-5.2.6_x64.msi — esante.gouv.fr',                url: CRYPTOLIB_URL,  btnLabel: '⬇️ Télécharger Cryptolib CPS 5.2.6', successMsg: 'Le téléchargement a été ouvert dans votre navigateur. Installez le fichier .msi une fois téléchargé.' })
    if (cnamOutdated)                          addDownloadBlock(footer, { icon: '📦', label: 'Mettre à jour les Services CNAM',       cmd: 'espacepro.ameli.fr — Section Aide',                           url: CNAM_URL,       btnLabel: '⬇️ Télécharger les Services CNAM',  successMsg: 'La page de téléchargement AmeliPro a été ouverte dans votre navigateur.' })
    if (extensionMissingBrowser)               addExtensionDownloadBlock(footer, extensionMissingBrowser)
    if (browserOutdated && browserSlugForUpdate) addBrowserUpdateBlock(footer, browserSlugForUpdate)
    if (usbSuspendActive)                      addUsbSuspendBlock(footer)

    const homeBtn = document.createElement('button')
    homeBtn.className = 'btn-dr-secondary mt-3'
    homeBtn.innerHTML = '🏠 Retour à l\'accueil'
    homeBtn.onclick = () => navigate('welcome')
    footer.appendChild(homeBtn)
  })
}

// ── Shared helpers ────────────────────────────────────────────────────────────

function badgeHtml({ text, color }) {
  const cls = color === 'success' ? 'dr-badge-success'
            : color === 'warning' ? 'dr-badge-warning'
            : color === 'danger'  ? 'dr-badge-danger'
            : color === 'info'    ? 'dr-badge-info'
            : 'dr-badge-subtle'
  return `<span class="dr-badge ${cls}">${text}</span>`
}

function addCheck(list, label, detail) {
  const item = document.createElement('div')
  item.className = 'dr-check status-running fade-up'
  item.innerHTML = `
    <div class="dr-check-icon"><span class="dr-spinner"></span></div>
    <div class="dr-check-body">
      <div class="dr-check-label">${label}</div>
      <div class="dr-check-detail">${detail}</div>
    </div>`
  list.appendChild(item)
  return item
}

function setCheck(item, status, icon, label, detail, badge) {
  item.className = `dr-check status-${status}`
  item.innerHTML = `
    <div class="dr-check-icon">${icon}</div>
    <div class="dr-check-body">
      <div class="dr-check-label">${label} ${badge ? badgeHtml(badge) : ''}</div>
      <div class="dr-check-detail">${detail}</div>
    </div>`
}

function makeRepairBlock(area, { icon, label, cmd }, prepend = false) {
  const block = document.createElement('div')
  block.className = 'dr-repair fade-up'
  block.innerHTML = `
    <div class="dr-repair-info">
      <span class="dr-repair-icon">${icon}</span>
      <div>
        <div class="dr-repair-label">${label}</div>
        <div class="dr-repair-cmd">${cmd}</div>
      </div>
    </div>
    <button class="btn-dr-primary repair-btn">${icon} ${label}</button>
    <div class="dr-repair-result hidden"></div>
  `
  if (prepend) area.insertBefore(block, area.firstChild)
  else area.appendChild(block)
  return block
}

// ── URL-open blocks (download/link) ───────────────────────────────────────────

const CRYPTOLIB_URL = 'https://esante.gouv.fr/sites/default/files/media/document/CryptolibCPS-5.2.6_x64.msi'
const CNAM_URL      = 'https://espacepro.ameli.fr/inscription/#/aide#blocs-2'

function addDownloadBlock(area, { icon, label, cmd, url, btnLabel, successMsg }) {
  const block = makeRepairBlock(area, { icon, label, cmd }, true)
  const btn    = block.querySelector('.repair-btn')
  const result = block.querySelector('.dr-repair-result')
  btn.innerHTML = btnLabel

  btn.addEventListener('click', async () => {
    btn.disabled = true
    btn.innerHTML = '<span class="dr-spinner"></span> Ouverture…'
    try {
      await invoke('open_url', { url })
      result.className = 'dr-repair-result dr-check status-success fade-up'
      result.innerHTML = `<div class="dr-check-icon">✅</div><div class="dr-check-body"><div class="dr-check-detail">${successMsg}</div></div>`
      btn.innerHTML = '✅ Ouvert'
    } catch (e) {
      result.className = 'dr-repair-result dr-check status-warning fade-up'
      result.innerHTML = `<div class="dr-check-icon">⚠️</div><div class="dr-check-body"><div class="dr-check-detail">Impossible d'ouvrir le lien : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = btnLabel
    }
  })
}

// ── Extension download block ──────────────────────────────────────────────────

const EXTENSION_URLS = {
  chrome:  'https://chromewebstore.google.com/detail/lecture-carte-vitale/kpjpglcbcgnblkigbedgaoegjbifejka?hl=fr',
  firefox: 'https://addons.mozilla.org/fr/firefox/addon/lecture-carte-vitale/',
  edge:    'https://chromewebstore.google.com/detail/lecture-carte-vitale/kpjpglcbcgnblkigbedgaoegjbifejka',
}

function addExtensionDownloadBlock(area, browser) {
  const url         = EXTENSION_URLS[browser]
  const browserName = browser === 'chrome' ? 'Google Chrome' : browser === 'firefox' ? 'Mozilla Firefox' : 'Microsoft Edge'
  const store       = browser === 'firefox' ? 'Firefox Add-ons' : 'Chrome Web Store'
  addDownloadBlock(area, {
    icon: '🧩', label: 'Installer l\'extension Lecture Carte Vitale',
    cmd: `${store} — ${browserName}`, url,
    btnLabel: '🧩 Installer l\'extension',
    successMsg: `La page ${store} a été ouverte dans votre navigateur. Cliquez sur "Ajouter" pour installer l'extension.`
  })
}

// ── Browser update block ──────────────────────────────────────────────────────

function addBrowserUpdateBlock(area, browser) {
  const browserName = browser === 'chrome' ? 'Google Chrome' : browser === 'firefox' ? 'Mozilla Firefox' : 'Microsoft Edge'
  const block = makeRepairBlock(area, {
    icon: '🔄', label: `Mettre à jour ${browserName}`,
    cmd: 'Ouvre la page de mise à jour intégrée du navigateur'
  }, true)

  const btn    = block.querySelector('.repair-btn')
  const result = block.querySelector('.dr-repair-result')

  btn.addEventListener('click', async () => {
    btn.disabled = true
    btn.innerHTML = '<span class="dr-spinner"></span> Ouverture…'
    try {
      const r = await invoke('launch_browser_update', { browser })
      if (r.ps_unavailable) {
        result.className = 'dr-repair-result dr-check status-warning fade-up'
        result.innerHTML = `<div class="dr-check-icon">⚠️</div><div class="dr-check-body"><div class="dr-check-detail">${r.detail}</div></div>`
        btn.disabled = false
        btn.innerHTML = '🔄 Réessayer'
      } else {
        result.className = 'dr-repair-result dr-check status-success fade-up'
        result.innerHTML = `<div class="dr-check-icon">✅</div><div class="dr-check-body"><div class="dr-check-detail">${r.detail}</div></div>`
        btn.innerHTML = `✅ ${browserName} ouvert`
      }
    } catch (e) {
      result.className = 'dr-repair-result dr-check status-warning fade-up'
      result.innerHTML = `<div class="dr-check-icon">⚠️</div><div class="dr-check-body"><div class="dr-check-detail">Impossible de lancer la mise à jour : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = '🔄 Réessayer'
    }
  })
}

// ── USB Selective Suspend disable block ───────────────────────────────────────

function addUsbSuspendBlock(area) {
  const block = makeRepairBlock(area, {
    icon: '🔌', label: 'Désactiver la mise en veille sélective USB',
    cmd: 'powercfg /SETACVALUEINDEX + /SETDCVALUEINDEX → 0 puis /SETACTIVE'
  }, true)

  const btn    = block.querySelector('.repair-btn')
  const result = block.querySelector('.dr-repair-result')

  btn.addEventListener('click', async () => {
    btn.disabled = true
    btn.innerHTML = '<span class="dr-spinner"></span> Application en cours…'
    result.className = 'dr-repair-result dr-check status-running fade-up'
    result.innerHTML = `<div class="dr-check-icon">⏳</div><div class="dr-check-body"><div class="dr-check-detail">Modification du plan d'alimentation…</div></div>`
    try {
      const r = await invoke('disable_usb_suspend')
      if (r.ps_unavailable) {
        result.className = 'dr-repair-result dr-check status-warning fade-up'
        result.innerHTML = `<div class="dr-check-icon">⚠️</div><div class="dr-check-body"><div class="dr-check-detail">${r.detail}</div></div>`
        btn.disabled = false
        btn.innerHTML = '🔌 Réessayer'
      } else {
        result.className = `dr-repair-result dr-check status-${r.is_ok ? 'success' : 'warning'} fade-up`
        result.innerHTML = `<div class="dr-check-icon">${r.is_ok ? '✅' : '⚠️'}</div><div class="dr-check-body"><div class="dr-check-detail">${r.detail}</div></div>`
        btn.innerHTML = r.is_ok ? '✅ Mise en veille USB désactivée' : '⚠️ Modification avec avertissements'
      }
    } catch (e) {
      result.className = 'dr-repair-result dr-check status-warning fade-up'
      result.innerHTML = `<div class="dr-check-icon">⚠️</div><div class="dr-check-body"><div class="dr-check-detail">La modification n'a pas pu être appliquée : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = '🔌 Réessayer'
    }
  })
}
