import { invoke } from '@tauri-apps/api/core'
import { getOsInfo, getRamInfo } from 'tauri-plugin-hwinfo'
import { notify } from '../notify.js'

const MIN_BUILD  = 26200
const MIN_RAM_MB = 16 * 1024

export function renderWindows(container, navigate) {
  container.innerHTML = `
    <button class="btn-dr-subtle mb-3" id="back-btn">← Accueil</button>

    <div class="dr-page-header">
      <h2><svg xmlns="http://www.w3.org/2000/svg" width="24" heigh="24" fill-rule="evenodd" clip-rule="evenodd" image-rendering="optimizeQuality" shape-rendering="geometricPrecision" text-rendering="geometricPrecision" viewBox="0 0 512 512.02">
            <path fill="#0078d4" fill-rule="nonzero" d="M0 512.02h242.686V269.335H0zm0-269.334h242.686V0H0zm269.314 0H512V0H269.314zm0 269.334H512V269.335H269.314z"/>
          </svg> Windows &amp; Maintenance</h2>
      <p>Vérification de la version du système, intégrité des fichiers et santé du disque</p>
    </div>

    <div class="dr-btn-row">
      <button class="btn-dr-primary" id="launch-btn">▶ Lancer la vérification</button>
      <button class="btn-dr-secondary" id="cancel-btn" style="display:none">✕ Annuler</button>
    </div>

    <div class="dr-checks" id="checks-list"></div>
    <div class="dr-repairs" id="repair-actions"></div>
  `

  container.querySelector('#back-btn').onclick = () => navigate('welcome')

  const launchBtn  = container.querySelector('#launch-btn')
  const cancelBtn  = container.querySelector('#cancel-btn')
  const checksList = container.querySelector('#checks-list')
  const repairArea = container.querySelector('#repair-actions')

  let cancelled = false

  cancelBtn.addEventListener('click', () => {
    cancelled = true
    cancelBtn.disabled = true
    cancelBtn.innerHTML = 'Annulation en cours…'
  })

  launchBtn.addEventListener('click', async () => {
    cancelled = false
    cancelBtn.disabled = false
    cancelBtn.innerHTML = '✕ Annuler'
    cancelBtn.style.display = ''

    launchBtn.disabled = true
    launchBtn.innerHTML = '<span class="dr-spinner"></span> Vérification en cours…'
    checksList.innerHTML = ''
    repairArea.innerHTML = ''

    let versionError       = false
    let sfcError           = false
    let chkdskError        = false
    let avError            = false
    let winreError         = false
    let fastStartupDisabled = false
    let qmrDisabled        = false

    function wasCancelled(item, label) {
      if (!cancelled) return false
      setCheck(item, 'cancelled', '⊘', label,
        'Vérification annulée par l\'utilisateur.',
        { text: 'Annulé', color: 'subtle' })
      return true
    }

    // ── 0. Point de restauration ──────────────────────────────────────────────
    const rpItem = addCheck(checksList, 'Point de restauration Windows', 'Création d\'un point de restauration…')
    if (!wasCancelled(rpItem, 'Point de restauration Windows')) {
      try {
        const r = await invoke('create_restore_point')
        if (r.ps_unavailable) {
          setCheck(rpItem, 'warning', '⚠️', 'Point de restauration Windows',
            r.detail, { text: 'Indisponible', color: 'warning' })
        } else {
          setCheck(rpItem, r.is_ok ? 'success' : 'warning', r.is_ok ? '✅' : '⚠️',
            'Point de restauration Windows', r.detail,
            r.is_ok ? { text: 'Créé', color: 'success' } : { text: 'Non créé', color: 'warning' })
        }
      } catch (e) {
        setCheck(rpItem, 'warning', '⚠️', 'Point de restauration Windows',
          `Le point de restauration n'a pas pu être créé : ${e}`,
          { text: 'Indisponible', color: 'warning' })
      }
    }

    // ── 1. Windows version ────────────────────────────────────────────────────
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

    // ── 2. RAM ────────────────────────────────────────────────────────────────
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

    // ── 3. Storage type ───────────────────────────────────────────────────────
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

    // ── 4. WinRE ──────────────────────────────────────────────────────────────
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

    // ── 5. SFC ────────────────────────────────────────────────────────────────
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

    // ── 6. Nettoyage du disque ────────────────────────────────────────────────
    const dcItem = addCheck(checksList, 'Nettoyage du disque C:', 'Suppression des fichiers temporaires…')
    if (!wasCancelled(dcItem, 'Nettoyage du disque C:')) {
      try {
        const r = await invoke('run_disk_cleanup')
        if (r.ps_unavailable) {
          setCheck(dcItem, 'warning', '⚠️', 'Nettoyage du disque C:',
            r.detail, { text: 'Indisponible', color: 'warning' })
        } else {
          setCheck(dcItem, r.is_ok ? 'success' : 'warning', r.is_ok ? '✅' : '⚠️',
            'Nettoyage du disque C:', r.detail,
            r.is_ok ? { text: 'Effectué', color: 'success' } : { text: 'Erreur', color: 'warning' })
        }
      } catch (e) {
        setCheck(dcItem, 'warning', '⚠️', 'Nettoyage du disque C:',
          `Le nettoyage du disque n'a pas pu être lancé : ${e}`,
          { text: 'Indisponible', color: 'warning' })
      }
    }

    // ── 7. CHKDSK ─────────────────────────────────────────────────────────────
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

    // ── 8. Antivirus ──────────────────────────────────────────────────────────
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

    // ── 9. Démarrage rapide ───────────────────────────────────────────────────
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

    // ── 10. Santé de la batterie (portables uniquement) ───────────────────────
    const batItem = addCheck(checksList, 'Santé de la batterie', 'Détection du type de machine…')
    if (!wasCancelled(batItem, 'Santé de la batterie')) {
      try {
        const r = await invoke('check_battery_health')
        if (r.ps_unavailable) {
          setCheck(batItem, 'warning', '⚠️', 'Santé de la batterie',
            r.detail, { text: 'Indisponible', color: 'warning' })
        } else if (!r.is_laptop) {
          batItem.remove()
        } else if (!r.has_battery) {
          setCheck(batItem, 'warning', '⚠️', 'Santé de la batterie',
            r.detail, { text: 'Indisponible', color: 'warning' })
        } else {
          const pct    = r.health_pct
          const status = pct >= 80 ? 'success' : pct >= 50 ? 'warning' : 'error'
          const icon   = pct >= 80 ? '✅' : pct >= 50 ? '⚠️' : '❌'
          const bColor = pct >= 80 ? 'success' : pct >= 50 ? 'warning' : 'danger'
          setCheck(batItem, status, icon, 'Santé de la batterie',
            r.detail, { text: `${pct}%`, color: bColor })
        }
      } catch (e) {
        setCheck(batItem, 'warning', '⚠️', 'Santé de la batterie',
          `La vérification de la batterie n'a pas pu être lancée : ${e}`,
          { text: 'Indisponible', color: 'warning' })
      }
    }

    // ── 11. Récupération machine rapide (QMR) ─────────────────────────────────
    const qmrItem = addCheck(checksList, 'Récupération machine rapide (QMR)', 'Lecture de la configuration…')
    if (!wasCancelled(qmrItem, 'Récupération machine rapide (QMR)')) {
      try {
        const r = await invoke('check_qmr')
        if (r.not_found) {
          qmrItem.remove()
        } else if (r.ps_unavailable) {
          setCheck(qmrItem, 'warning', '⚠️', 'Récupération machine rapide (QMR)',
            r.detail, { text: 'Indisponible', color: 'warning' })
        } else {
          qmrDisabled = !r.is_ok
          setCheck(qmrItem, r.is_ok ? 'success' : 'warning', r.is_ok ? '✅' : '⚠️',
            'Récupération machine rapide (QMR)', r.detail,
            r.is_ok ? { text: 'Activée', color: 'success' } : { text: 'Désactivée', color: 'warning' })
        }
      } catch (e) {
        setCheck(qmrItem, 'warning', '⚠️', 'Récupération machine rapide (QMR)',
          `La vérification n'a pas pu être lancée : ${e}`,
          { text: 'Indisponible', color: 'warning' })
      }
    }

    // ── Done ──────────────────────────────────────────────────────────────────
    cancelBtn.style.display = 'none'
    launchBtn.disabled = false
    launchBtn.innerHTML = '🔄 Relancer la vérification'

    if (!cancelled) {
      const issueCount = [versionError, sfcError, chkdskError, avError, winreError, fastStartupDisabled, qmrDisabled].filter(Boolean).length
      if (issueCount === 0) {
        notify('Dr Reco — Windows', '✅ Vérification terminée — Aucun problème détecté.')
      } else {
        notify('Dr Reco — Windows', `⚠️ Vérification terminée — ${issueCount} problème${issueCount > 1 ? 's' : ''} détecté${issueCount > 1 ? 's' : ''}.`)
      }

      if (versionError)        addWindowsUpdateBlock(repairArea)
      if (sfcError)            addRepairBlock(repairArea, { icon: '🛠️', label: 'Réparer Windows',   cmd: 'dism /online /cleanup-image /restorehealth', kind: 'sfc' })
      if (chkdskError)         addRepairBlock(repairArea, { icon: '💾', label: 'Réparer le disque', cmd: 'chkdsk C: /f /r', kind: 'chkdsk' })
      if (avError)             addDefenderBlock(repairArea)
      if (winreError)          addWinreRepairBlock(repairArea)
      if (fastStartupDisabled) addFastStartupBlock(repairArea)
      if (qmrDisabled)         addQmrBlock(repairArea)
    }

    const homeBtn = document.createElement('button')
    homeBtn.className = 'btn-dr-secondary mt-3'
    homeBtn.innerHTML = '🏠 Retour à l\'accueil'
    homeBtn.onclick = () => navigate('welcome')
    repairArea.appendChild(homeBtn)
  })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

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

function makeRepairBlock(area, { icon, label, cmd }) {
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
  area.appendChild(block)
  return block
}

function wireRepairBtn(block, invokeCmd, invokeArgs, { successLabel, retryLabel }) {
  const btn    = block.querySelector('.repair-btn')
  const result = block.querySelector('.dr-repair-result')

  btn.addEventListener('click', async () => {
    btn.disabled = true
    btn.innerHTML = '<span class="dr-spinner"></span> En cours…'
    result.className = 'dr-repair-result dr-check status-running fade-up'
    result.innerHTML = `<div class="dr-check-icon">⏳</div><div class="dr-check-body"><div class="dr-check-detail">Exécution en cours, veuillez patienter…</div></div>`

    try {
      const r = await invokeCmd(invokeArgs)
      if (r.ps_unavailable) {
        result.className = 'dr-repair-result dr-check status-warning fade-up'
        result.innerHTML = `<div class="dr-check-icon">⚠️</div><div class="dr-check-body"><div class="dr-check-detail">${r.detail}</div></div>`
        btn.disabled = false
        btn.innerHTML = retryLabel
      } else {
        result.className = `dr-repair-result dr-check status-${r.is_ok ? 'success' : 'warning'} fade-up`
        result.innerHTML = `<div class="dr-check-icon">${r.is_ok ? '✅' : '⚠️'}</div><div class="dr-check-body"><div class="dr-check-detail">${r.detail}</div></div>`
        btn.innerHTML = r.is_ok ? successLabel : '⚠️ Terminé avec avertissements'
      }
    } catch (e) {
      result.className = 'dr-repair-result dr-check status-warning fade-up'
      result.innerHTML = `<div class="dr-check-icon">⚠️</div><div class="dr-check-body"><div class="dr-check-detail">Erreur : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = retryLabel
    }
  })
}

function addRepairBlock(area, { icon, label, cmd, kind }) {
  const block = makeRepairBlock(area, { icon, label, cmd })
  wireRepairBtn(block,
    (args) => invoke('run_repair', args), { kind },
    { successLabel: '✅ Réparation terminée', retryLabel: `${icon} Réessayer` }
  )
}

function addDefenderBlock(area) {
  const block = makeRepairBlock(area, {
    icon: '🛡️', label: 'Activer Microsoft Defender',
    cmd: 'Set-MpPreference -DisableRealtimeMonitoring $false'
  })
  wireRepairBtn(block,
    () => invoke('activate_defender'), {},
    { successLabel: '✅ Defender activé', retryLabel: '🛡️ Réessayer' }
  )
}

function addWinreRepairBlock(area) {
  const block = makeRepairBlock(area, {
    icon: '🔄', label: 'Réparer WinRE',
    cmd: 'reagentc /disable → reagentc /enable'
  })
  wireRepairBtn(block,
    () => invoke('repair_winre'), {},
    { successLabel: '✅ WinRE réparé', retryLabel: '🔄 Réessayer' }
  )
}

function addWindowsUpdateBlock(area) {
  const block = makeRepairBlock(area, {
    icon: '🔄', label: 'Lancer Windows Update',
    cmd: 'usoclient ScanInstallWait'
  })
  wireRepairBtn(block,
    () => invoke('launch_windows_update'), {},
    { successLabel: '✅ Windows Update lancé', retryLabel: '🔄 Réessayer' }
  )
}

function addFastStartupBlock(area) {
  const block = makeRepairBlock(area, {
    icon: '⚡', label: 'Activer le démarrage rapide',
    cmd: 'powercfg /hibernate on → HiberbootEnabled = 1'
  })
  wireRepairBtn(block,
    () => invoke('enable_fast_startup'), {},
    { successLabel: '✅ Démarrage rapide activé', retryLabel: '⚡ Réessayer' }
  )
}

function addQmrBlock(area) {
  const block = makeRepairBlock(area, {
    icon: '☁️', label: 'Activer la récupération machine rapide',
    cmd: 'reagentc /setrecoverysettings — CloudRemediation + AutoRemediation'
  })
  wireRepairBtn(block,
    () => invoke('enable_qmr'), {},
    { successLabel: '✅ Récupération machine rapide activée', retryLabel: '☁️ Réessayer' }
  )
}
