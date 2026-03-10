import { invoke } from '@tauri-apps/api/core'
import { getOsInfo, getRamInfo } from 'tauri-plugin-hwinfo'

const MIN_BUILD  = 26200
const MIN_RAM_MB = 16 * 1024

export function renderWindows(container, navigate) {
  container.innerHTML = `
    <fluent-button appearance="subtle" id="back-btn">← Accueil</fluent-button>

    <div class="page-header" style="margin-top:12px">
      <h2>🪟 Windows &amp; Maintenance</h2>
      <p>Vérification de la version du système, intégrité des fichiers et santé du disque</p>
    </div>

    <div class="btn-row">
      <fluent-button appearance="primary" id="launch-btn">▶ Lancer la vérification</fluent-button>
      <fluent-button appearance="secondary" id="cancel-btn" style="display:none">✕ Annuler</fluent-button>
    </div>

    <div class="checks-list"    id="checks-list"></div>
    <div class="repair-actions" id="repair-actions"></div>
  `

  container.querySelector('#back-btn').onclick = () => navigate('welcome')

  const launchBtn  = container.querySelector('#launch-btn')
  const cancelBtn  = container.querySelector('#cancel-btn')
  const checksList = container.querySelector('#checks-list')
  const repairArea = container.querySelector('#repair-actions')

  // Cancellation token — replaced on each run
  let cancelled = false

  cancelBtn.addEventListener('click', () => {
    cancelled = true
    cancelBtn.disabled = true
    cancelBtn.innerHTML = 'Annulation en cours…'
  })

  launchBtn.addEventListener('click', async () => {
    // Reset state
    cancelled = false
    cancelBtn.disabled = false
    cancelBtn.innerHTML = '✕ Annuler'
    cancelBtn.style.display = ''

    launchBtn.disabled = true
    launchBtn.innerHTML = '<fluent-spinner size="tiny" style="margin-right:8px"></fluent-spinner> Vérification en cours…'
    checksList.innerHTML = ''
    repairArea.innerHTML = ''

    let versionError = false
    let sfcError     = false
    let fastStartupDisabled = false
    let chkdskError  = false
    let avError      = false
    let winreError   = false

    // Helper: check cancellation before each step
    // Returns true if cancelled (caller should break)
    function wasCancelled(item, label) {
      if (!cancelled) return false
      setCheck(item, 'cancelled', '⊘', label,
        'Vérification annulée par l\'utilisateur.',
        { text: 'Annulé', color: 'subtle' })
      return true
    }

    // ── 1. Windows version ───────────────────────────────────────────────────
    const vItem = addCheck(checksList, 'Version de Windows', 'Récupération des informations système…')
    if (!wasCancelled(vItem, 'Version de Windows')) {
      try {
        const osInfo = await getOsInfo()
        const parts  = (osInfo.version || '').split('.')
        const build  = parseInt(parts[2] ?? '0', 10)
        const ok     = build >= MIN_BUILD
        versionError = !ok
        setCheck(vItem, ok ? 'success' : 'warning', ok ? '✅' : '⚠️',
          'Version de Windows',
          ok ? `${osInfo.name} (build ${build}) — Version conforme.`
             : `${osInfo.name} (build ${build}) — Windows 11 25H2 (build ${MIN_BUILD}) requis. Mettez à jour via Windows Update.`,
          ok ? { text: 'Conforme', color: 'success' } : { text: 'Mise à jour requise', color: 'warning' })
      } catch (e) {
        setCheck(vItem, 'warning', '⚠️', 'Version de Windows',
          `Impossible de récupérer la version du système : ${e}`,
          { text: 'Indisponible', color: 'warning' })
      }
    }

    // ── 2. RAM ───────────────────────────────────────────────────────────────
    const rItem = addCheck(checksList, 'Mémoire vive (RAM)', 'Récupération de la taille mémoire…')
    if (!wasCancelled(rItem, 'Mémoire vive (RAM)')) {
      try {
        const ramInfo = await getRamInfo()
        const sizeMb  = ramInfo.sizeMb ?? 0
        const sizeGb  = (sizeMb / 1024).toFixed(1)
        const ok      = sizeMb >= MIN_RAM_MB
        setCheck(rItem, ok ? 'success' : 'warning', ok ? '✅' : '⚠️',
          'Mémoire vive (RAM)',
          ok ? `${sizeGb} Go détectés — Capacité suffisante.`
             : `${sizeGb} Go détectés — Mémoire vive insuffisante. 16 Go minimum recommandés.`,
          ok ? { text: `${sizeGb} Go`, color: 'success' } : { text: 'Mémoire vive insuffisante', color: 'warning' })
      } catch (e) {
        setCheck(rItem, 'warning', '⚠️', 'Mémoire vive (RAM)',
          `Impossible de récupérer les informations mémoire : ${e}`,
          { text: 'Indisponible', color: 'warning' })
      }
    }

    // ── 3. Storage type (SSD vs HDD) ─────────────────────────────────────────
    const stItem = addCheck(checksList, 'Type de stockage', 'Détection du type de disque (SSD/HDD)…')
    if (!wasCancelled(stItem, 'Type de stockage')) {
      try {
        const r = await invoke('check_storage_type')
        if (r.ps_unavailable) {
          setCheck(stItem, 'warning', '⚠️', 'Type de stockage',
            r.detail, { text: 'PowerShell indisponible', color: 'warning' })
        } else {
          setCheck(stItem, r.is_ok ? 'success' : 'warning', r.is_ok ? '✅' : '⚠️',
            'Type de stockage', r.detail,
            r.is_ok ? { text: 'SSD', color: 'success' } : { text: 'HDD détecté', color: 'warning' })
        }
      } catch (e) {
        setCheck(stItem, 'warning', '⚠️', 'Type de stockage',
          `La détection du type de stockage n'a pas pu être lancée : ${e}`,
          { text: 'Indisponible', color: 'warning' })
      }
    }

    // ── 4. WinRE ─────────────────────────────────────────────────────────────
    const winreItem = addCheck(checksList, 'Windows Recovery (WinRE)', 'Vérification de l\'état de la partition de récupération…')
    if (!wasCancelled(winreItem, 'Windows Recovery (WinRE)')) {
      try {
        const r = await invoke('check_winre')
        if (r.ps_unavailable) {
          setCheck(winreItem, 'warning', '⚠️', 'Windows Recovery (WinRE)',
            r.detail, { text: 'Indisponible', color: 'warning' })
        } else {
          winreError = !r.is_ok
          setCheck(winreItem, r.is_ok ? 'success' : 'warning', r.is_ok ? '✅' : '⚠️',
            'Windows Recovery (WinRE)', r.detail,
            r.is_ok ? { text: 'Activé', color: 'success' } : { text: 'Désactivé', color: 'warning' })
        }
      } catch (e) {
        setCheck(winreItem, 'warning', '⚠️', 'Windows Recovery (WinRE)',
          `La vérification WinRE n'a pas pu être lancée : ${e}`,
          { text: 'Indisponible', color: 'warning' })
      }
    }

    // ── 5. SFC ───────────────────────────────────────────────────────────────
    const sItem = addCheck(checksList, 'Intégrité des fichiers système (SFC)', 'Analyse en cours — peut prendre plusieurs minutes…')
    if (!wasCancelled(sItem, 'Intégrité des fichiers système (SFC)')) {
      try {
        const r = await invoke('run_sfc_check')
        if (r.ps_unavailable) {
          setCheck(sItem, 'warning', '⚠️', 'Intégrité des fichiers système (SFC)',
            r.detail, { text: 'PowerShell indisponible', color: 'warning' })
        } else {
          sfcError = !r.is_ok
          setCheck(sItem, r.is_ok ? 'success' : 'warning', r.is_ok ? '✅' : '🔧',
            'Intégrité des fichiers système (SFC)', r.detail,
            r.is_ok ? { text: 'OK', color: 'success' } : { text: 'Erreur détectée', color: 'warning' })
        }
      } catch (e) {
        setCheck(sItem, 'warning', '⚠️', 'Intégrité des fichiers système (SFC)',
          `La vérification SFC n'a pas pu être lancée : ${e}`,
          { text: 'Indisponible', color: 'warning' })
      }
    }

    // ── 6. CHKDSK ────────────────────────────────────────────────────────────
    const cItem = addCheck(checksList, 'Santé du disque C: (CHKDSK)', 'Analyse du disque en cours…')
    if (!wasCancelled(cItem, 'Santé du disque C: (CHKDSK)')) {
      try {
        const r = await invoke('run_chkdsk')
        if (r.ps_unavailable) {
          setCheck(cItem, 'warning', '⚠️', 'Santé du disque C: (CHKDSK)',
            r.detail, { text: 'PowerShell indisponible', color: 'warning' })
        } else {
          chkdskError = !r.is_ok
          setCheck(cItem, r.is_ok ? 'success' : 'warning', r.is_ok ? '✅' : '🔧',
            'Santé du disque C: (CHKDSK)', r.detail,
            r.is_ok ? { text: 'OK', color: 'success' } : { text: 'Erreur détectée', color: 'warning' })
        }
      } catch (e) {
        setCheck(cItem, 'warning', '⚠️', 'Santé du disque C: (CHKDSK)',
          `La vérification CHKDSK n'a pas pu être lancée : ${e}`,
          { text: 'Indisponible', color: 'warning' })
      }
    }

    // ── 7. Antivirus ─────────────────────────────────────────────────────────
    const avItem = addCheck(checksList, 'Antivirus', 'Interrogation du centre de sécurité Windows…')
    if (!wasCancelled(avItem, 'Antivirus')) {
      try {
        const av = await invoke('check_antivirus')
        if (av.ps_unavailable) {
          setCheck(avItem, 'warning', '⚠️', 'Antivirus',
            'Impossible d\'interroger le centre de sécurité Windows — PowerShell est indisponible sur ce système.',
            { text: 'PowerShell indisponible', color: 'warning' })
        } else if (av.active.length > 0) {
          setCheck(avItem, 'success', '✅', 'Antivirus',
            `Antivirus actif : ${av.active.join(', ')}.`,
            { text: av.active[0], color: 'success' })
        } else if (av.inactive.length > 0) {
          avError = true
          setCheck(avItem, 'warning', '⚠️', 'Antivirus',
            `Aucun antivirus en cours de fonctionnement. Installé mais inactif : ${av.inactive.join(', ')}.`,
            { text: 'Inactif', color: 'warning' })
        } else {
          avError = true
          setCheck(avItem, 'error', '❌', 'Antivirus',
            'Aucun antivirus en cours de fonctionnement.',
            { text: 'Non détecté', color: 'danger' })
        }
      } catch (e) {
        setCheck(avItem, 'warning', '⚠️', 'Antivirus',
          `La vérification antivirus n'a pas pu être lancée : ${e}`,
          { text: 'Indisponible', color: 'warning' })
      }
    }

    // ── 8. Démarrage rapide ───────────────────────────────────────────────────
    const fsItem = addCheck(checksList, 'Démarrage rapide Windows', 'Lecture du registre…')
    if (!wasCancelled(fsItem, 'Démarrage rapide Windows')) {
      try {
        const r = await invoke('check_fast_startup')
        fastStartupDisabled = !r.is_ok
        setCheck(fsItem, r.is_ok ? 'success' : 'warning', r.is_ok ? '✅' : '⚠️',
          'Démarrage rapide Windows', r.detail,
          r.is_ok ? { text: 'Activé', color: 'success' } : { text: 'Désactivé', color: 'warning' })
      } catch (e) {
        setCheck(fsItem, 'warning', '⚠️', 'Démarrage rapide Windows',
          `La vérification n'a pas pu être lancée : ${e}`,
          { text: 'Indisponible', color: 'warning' })
      }
    }

    // ── Done ─────────────────────────────────────────────────────────────────
    cancelBtn.style.display = 'none'
    launchBtn.disabled = false
    launchBtn.innerHTML = '🔄 Relancer la vérification'

    if (!cancelled) {
      if (versionError) addWindowsUpdateBlock(repairArea)
      if (sfcError)     addRepairBlock(repairArea, { icon: '🛠️', label: 'Réparer Windows',   detail: 'dism /online /cleanup-image /restorehealth', kind: 'sfc' })
      if (chkdskError)  addRepairBlock(repairArea, { icon: '💾', label: 'Réparer le disque', detail: 'chkdsk C: /f /r', kind: 'chkdsk' })
      if (avError)      addDefenderBlock(repairArea)
      if (winreError)   addWinreRepairBlock(repairArea)
      if (fastStartupDisabled) addFastStartupBlock(repairArea)
    }

    const homeBtn = document.createElement('fluent-button')
    homeBtn.setAttribute('appearance', 'secondary')
    homeBtn.style.marginTop = '16px'
    homeBtn.innerHTML = '🏠 Retour à l\'accueil'
    homeBtn.onclick = () => navigate('welcome')
    repairArea.appendChild(homeBtn)
  })
}

// ── Check item helpers ────────────────────────────────────────────────────────
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

// ── Repair block helper ───────────────────────────────────────────────────────
function addRepairBlock(area, { icon, label, detail, kind }) {
  const block = document.createElement('div')
  block.className = 'repair-block fade-up'
  block.innerHTML = `
    <div class="repair-info">
      <span class="repair-icon">${icon}</span>
      <div>
        <div class="repair-label">${label}</div>
        <div class="repair-detail">${detail}</div>
      </div>
    </div>
    <fluent-button appearance="primary" class="repair-btn">${icon} ${label}</fluent-button>
    <div class="repair-result hidden"></div>
  `
  area.appendChild(block)

  const btn    = block.querySelector('.repair-btn')
  const result = block.querySelector('.repair-result')

  btn.addEventListener('click', async () => {
    btn.disabled = true
    btn.innerHTML = '<fluent-spinner size="tiny" style="margin-right:8px"></fluent-spinner> Réparation en cours…'
    result.className = 'repair-result check-item status-running fade-up'
    result.innerHTML = `<div class="check-icon">⏳</div><div class="check-body"><div class="check-detail">Exécution en cours, veuillez patienter…</div></div>`

    try {
      const r = await invoke('run_repair', { kind })
      if (r.ps_unavailable) {
        result.className = 'repair-result check-item status-warning fade-up'
        result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">${r.detail}</div></div>`
        btn.disabled = false
        btn.innerHTML = `${icon} Réessayer`
      } else {
        result.className = `repair-result check-item status-${r.is_ok ? 'success' : 'warning'} fade-up`
        result.innerHTML = `
          <div class="check-icon">${r.is_ok ? '✅' : '⚠️'}</div>
          <div class="check-body"><div class="check-detail">${r.detail}</div></div>`
        btn.innerHTML = r.is_ok ? '✅ Réparation terminée' : '⚠️ Terminée avec avertissements'
      }
    } catch (e) {
      result.className = 'repair-result check-item status-warning fade-up'
      result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">La réparation n'a pas pu être lancée : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = `${icon} Réessayer`
    }
  })
}

// ── Defender activation block ─────────────────────────────────────────────────
function addDefenderBlock(area) {
  const block = document.createElement('div')
  block.className = 'repair-block fade-up'
  block.innerHTML = `
    <div class="repair-info">
      <span class="repair-icon">🛡️</span>
      <div>
        <div class="repair-label">Activer Microsoft Defender</div>
        <div class="repair-detail">Set-MpPreference -DisableRealtimeMonitoring $false</div>
      </div>
    </div>
    <fluent-button appearance="primary" class="defender-btn">🛡️ Activer Microsoft Defender</fluent-button>
    <div class="defender-result hidden"></div>
  `
  area.appendChild(block)

  const btn    = block.querySelector('.defender-btn')
  const result = block.querySelector('.defender-result')

  btn.addEventListener('click', async () => {
    btn.disabled = true
    btn.innerHTML = '<fluent-spinner size="tiny" style="margin-right:8px"></fluent-spinner> Activation en cours…'
    result.className = 'defender-result check-item status-running fade-up'
    result.innerHTML = `<div class="check-icon">⏳</div><div class="check-body"><div class="check-detail">Activation de Microsoft Defender…</div></div>`

    try {
      const r = await invoke('activate_defender')
      if (r.ps_unavailable) {
        result.className = 'defender-result check-item status-warning fade-up'
        result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">${r.detail}</div></div>`
        btn.disabled = false
        btn.innerHTML = '🛡️ Réessayer'
      } else {
        result.className = `defender-result check-item status-${r.is_ok ? 'success' : 'warning'} fade-up`
        result.innerHTML = `
          <div class="check-icon">${r.is_ok ? '✅' : '⚠️'}</div>
          <div class="check-body"><div class="check-detail">${r.detail}</div></div>`
        btn.innerHTML = r.is_ok ? '✅ Defender activé' : '⚠️ Activation avec avertissements'
      }
    } catch (e) {
      result.className = 'defender-result check-item status-warning fade-up'
      result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">L'activation n'a pas pu être lancée : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = '🛡️ Réessayer'
    }
  })
}

// ── WinRE repair block ────────────────────────────────────────────────────────
function addWinreRepairBlock(area) {
  const block = document.createElement('div')
  block.className = 'repair-block fade-up'
  block.innerHTML = `
    <div class="repair-info">
      <span class="repair-icon">🔄</span>
      <div>
        <div class="repair-label">Réparer WinRE</div>
        <div class="repair-detail">reagentc /disable → reagentc /enable</div>
      </div>
    </div>
    <fluent-button appearance="primary" class="winre-btn">🔄 Réparer WinRE</fluent-button>
    <div class="winre-result hidden"></div>
  `
  area.appendChild(block)

  const btn    = block.querySelector('.winre-btn')
  const result = block.querySelector('.winre-result')

  btn.addEventListener('click', async () => {
    btn.disabled = true
    btn.innerHTML = '<fluent-spinner size="tiny" style="margin-right:8px"></fluent-spinner> Réparation en cours…'
    result.className = 'winre-result check-item status-running fade-up'
    result.innerHTML = `<div class="check-icon">⏳</div><div class="check-body"><div class="check-detail">Exécution de reagentc /disable puis /enable…</div></div>`

    try {
      const r = await invoke('repair_winre')
      if (r.ps_unavailable) {
        result.className = 'winre-result check-item status-warning fade-up'
        result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">${r.detail}</div></div>`
        btn.disabled = false
        btn.innerHTML = '🔄 Réessayer'
      } else {
        result.className = `winre-result check-item status-${r.is_ok ? 'success' : 'warning'} fade-up`
        result.innerHTML = `
          <div class="check-icon">${r.is_ok ? '✅' : '⚠️'}</div>
          <div class="check-body"><div class="check-detail">${r.detail}</div></div>`
        btn.innerHTML = r.is_ok ? '✅ WinRE réparé' : '⚠️ Réparation avec avertissements'
      }
    } catch (e) {
      result.className = 'winre-result check-item status-warning fade-up'
      result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">La réparation WinRE n'a pas pu être lancée : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = '🔄 Réessayer'
    }
  })
}

// ── Windows Update action block ───────────────────────────────────────────────
function addWindowsUpdateBlock(area) {
  const block = document.createElement('div')
  block.className = 'repair-block fade-up'
  block.innerHTML = `
    <div class="repair-info">
      <span class="repair-icon">🔄</span>
      <div>
        <div class="repair-label">Lancer Windows Update</div>
        <div class="repair-detail">usoclient ScanInstallWait</div>
      </div>
    </div>
    <fluent-button appearance="primary" class="wu-btn">🔄 Lancer Windows Update</fluent-button>
    <div class="wu-result hidden"></div>
  `
  area.appendChild(block)

  const btn    = block.querySelector('.wu-btn')
  const result = block.querySelector('.wu-result')

  btn.addEventListener('click', async () => {
    btn.disabled = true
    btn.innerHTML = '<fluent-spinner size="tiny" style="margin-right:8px"></fluent-spinner> Lancement en cours…'
    result.className = 'wu-result check-item status-running fade-up'
    result.innerHTML = `<div class="check-icon">⏳</div><div class="check-body"><div class="check-detail">Démarrage de Windows Update en arrière-plan…</div></div>`

    try {
      const r = await invoke('launch_windows_update')
      if (r.ps_unavailable) {
        result.className = 'wu-result check-item status-warning fade-up'
        result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">${r.detail}</div></div>`
        btn.disabled = false
        btn.innerHTML = '🔄 Réessayer'
      } else {
        result.className = `wu-result check-item status-${r.is_ok ? 'success' : 'warning'} fade-up`
        result.innerHTML = `
          <div class="check-icon">${r.is_ok ? '✅' : '⚠️'}</div>
          <div class="check-body"><div class="check-detail">${r.detail}</div></div>`
        btn.innerHTML = r.is_ok ? '✅ Windows Update lancé' : '⚠️ Lancé avec avertissements'
      }
    } catch (e) {
      result.className = 'wu-result check-item status-warning fade-up'
      result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">Windows Update n'a pas pu être lancé : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = '🔄 Réessayer'
    }
  })
}

// ── Fast Startup enable block ─────────────────────────────────────────────────
function addFastStartupBlock(area) {
  const block = document.createElement('div')
  block.className = 'repair-block fade-up'
  block.innerHTML = `
    <div class="repair-info">
      <span class="repair-icon">⚡</span>
      <div>
        <div class="repair-label">Activer le démarrage rapide</div>
        <div class="repair-detail">powercfg /hibernate on → HiberbootEnabled = 1</div>
      </div>
    </div>
    <fluent-button appearance="primary" class="fs-btn">⚡ Activer le démarrage rapide</fluent-button>
    <div class="fs-result hidden"></div>
  `
  area.appendChild(block)

  const btn    = block.querySelector('.fs-btn')
  const result = block.querySelector('.fs-result')

  btn.addEventListener('click', async () => {
    btn.disabled = true
    btn.innerHTML = '<fluent-spinner size="tiny" style="margin-right:8px"></fluent-spinner> Activation en cours…'
    result.className = 'fs-result check-item status-running fade-up'
    result.innerHTML = `<div class="check-icon">⏳</div><div class="check-body"><div class="check-detail">Activation du démarrage rapide…</div></div>`

    try {
      const r = await invoke('enable_fast_startup')
      if (r.ps_unavailable) {
        result.className = 'fs-result check-item status-warning fade-up'
        result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">${r.detail}</div></div>`
        btn.disabled = false
        btn.innerHTML = '⚡ Réessayer'
      } else {
        result.className = `fs-result check-item status-${r.is_ok ? 'success' : 'warning'} fade-up`
        result.innerHTML = `
          <div class="check-icon">${r.is_ok ? '✅' : '⚠️'}</div>
          <div class="check-body"><div class="check-detail">${r.detail}</div></div>`
        btn.innerHTML = r.is_ok ? '✅ Démarrage rapide activé' : '⚠️ Activation avec avertissements'
      }
    } catch (e) {
      result.className = 'fs-result check-item status-warning fade-up'
      result.innerHTML = `<div class="check-icon">⚠️</div><div class="check-body"><div class="check-detail">L'activation n'a pas pu être lancée : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = '⚡ Réessayer'
    }
  })
}
