import { invoke } from '@tauri-apps/api/core'
import { getOsInfo, getRamInfo } from 'tauri-plugin-hwinfo'

const MIN_BUILD  = 26200
const MIN_RAM_MB = 15 * 1024

export function renderWindows(container, navigate) {
  container.innerHTML = `
    <fluent-button appearance="transparent" id="back-btn">← Accueil</fluent-button>

    <div class="page-header" style="margin-top:24px">
      <h2>🪟 Windows &amp; Maintenance</h2>
      <p>Vérification de la version du système, intégrité des fichiers et santé du disque</p>
    </div>

    <fluent-button appearance="primary" id="launch-btn">Démarrer la vérification</fluent-button>

    <div class="checks-list" id="checks-list"></div>
    <div class="repair-actions" id="repair-actions"></div>
  `

  container.querySelector('#back-btn').onclick = () => navigate('welcome')

  const launchBtn   = container.querySelector('#launch-btn')
  const checksList  = container.querySelector('#checks-list')
  const repairArea  = container.querySelector('#repair-actions')

  launchBtn.addEventListener('click', async () => {
    launchBtn.disabled = true
    launchBtn.innerHTML = '<fluent-spinner size="tiny" style="margin-right:8px"></fluent-spinner> Vérification en cours…'
    checksList.innerHTML = ''
    repairArea.innerHTML = ''

    let sfcError    = false
    let chkdskError = false
    let avError     = false

    // ── 1. Windows version ───────────────────────────────────────────────────
    const vItem = addCheck(checksList, 'Version de Windows', 'Récupération des informations système…', 'running')
    try {
      const osInfo = await getOsInfo()
      const parts  = (osInfo.version || '').split('.')
      const build  = parseInt(parts[2] ?? '0', 10)
      const ok     = build >= MIN_BUILD
      setCheck(vItem, ok ? 'success' : 'warning', ok ? '✅' : '⚠️', 'Version de Windows',
        ok ? `${osInfo.name} (build ${build}) — Version conforme.`
           : `${osInfo.name} (build ${build}) — Windows 11 25H2 (build ${MIN_BUILD}) requis. Mettez à jour via Windows Update.`,
        ok ? { text: 'Conforme', color: 'success' } : { text: 'Mise à jour requise', color: 'warning' })
    } catch (e) {
      setCheck(vItem, 'error', '❌', 'Version de Windows', 'Erreur : ' + e, { text: 'Erreur', color: 'danger' })
    }

    // ── 2. RAM ───────────────────────────────────────────────────────────────
    const rItem = addCheck(checksList, 'Mémoire vive (RAM)', 'Récupération de la taille mémoire…', 'running')
    try {
      const ramInfo = await getRamInfo()
      const sizeMb  = ramInfo.sizeMb ?? 0
      const sizeGb  = (sizeMb / 1024).toFixed(1)
      const ok      = sizeMb >= MIN_RAM_MB
      setCheck(rItem, ok ? 'success' : 'warning', ok ? '✅' : '⚠️', 'Mémoire vive (RAM)',
        ok ? `${sizeGb} Go détectés — Capacité suffisante.`
           : `${sizeGb} Go détectés — Mémoire vive insuffisante. 16 Go minimum recommandés.`,
        ok ? { text: `${sizeGb} Go`, color: 'success' } : { text: 'Mémoire vive insuffisante', color: 'warning' })
    } catch (e) {
      setCheck(rItem, 'error', '❌', 'Mémoire vive (RAM)', 'Erreur : ' + e, { text: 'Erreur', color: 'danger' })
    }

    // ── 3. SFC ───────────────────────────────────────────────────────────────
    const sItem = addCheck(checksList, 'Intégrité des fichiers système (SFC)', 'Analyse en cours — peut prendre plusieurs minutes…', 'running')
    try {
      const r = await invoke('run_sfc_check')
      sfcError = !r.is_ok
      setCheck(sItem, r.is_ok ? 'success' : 'warning', r.is_ok ? '✅' : '🔧',
        'Intégrité des fichiers système (SFC)', r.detail,
        r.is_ok ? { text: 'OK', color: 'success' } : { text: 'Erreur détectée', color: 'warning' })
    } catch (e) {
      sfcError = true
      setCheck(sItem, 'error', '❌', 'Intégrité des fichiers système (SFC)', 'Erreur : ' + e, { text: 'Erreur', color: 'danger' })
    }

    // ── 4. CHKDSK ────────────────────────────────────────────────────────────
    const cItem = addCheck(checksList, 'Santé du disque C: (CHKDSK)', 'Analyse du disque en cours…', 'running')
    try {
      const r = await invoke('run_chkdsk')
      chkdskError = !r.is_ok
      setCheck(cItem, r.is_ok ? 'success' : 'warning', r.is_ok ? '✅' : '🔧',
        'Santé du disque C: (CHKDSK)', r.detail,
        r.is_ok ? { text: 'OK', color: 'success' } : { text: 'Erreur détectée', color: 'warning' })
    } catch (e) {
      chkdskError = true
      setCheck(cItem, 'error', '❌', 'Santé du disque C: (CHKDSK)', 'Erreur : ' + e, { text: 'Erreur', color: 'danger' })
    }


    // ── 5. Antivirus ─────────────────────────────────────────────────────────
    const avItem = addCheck(checksList, 'Antivirus', 'Interrogation du centre de sécurité Windows…', 'running')
    try {
      const av = await invoke('check_antivirus')
      if (av.active.length > 0) {
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
      setCheck(avItem, 'error', '❌', 'Antivirus', 'Erreur : ' + e, { text: 'Erreur', color: 'danger' })
    }

    // ── Done ─────────────────────────────────────────────────────────────────
    launchBtn.disabled = false
    launchBtn.innerHTML = '🔄 Relancer la vérification'

    if (sfcError) {
      addRepairBlock(repairArea, { icon: '🛠️', label: 'Réparer Windows', detail: 'dism /online /cleanup-image /restorehealth', kind: 'sfc' })
    }
    if (chkdskError) {
      addRepairBlock(repairArea, { icon: '💾', label: 'Réparer le disque', detail: 'chkdsk C: /f /r', kind: 'chkdsk' })
    }

    if (avError) {
      addDefenderBlock(repairArea)
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
function addCheck(list, label, detail, status) {
  const item = document.createElement('div')
  item.className = `check-item status-${status} fade-up`
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
      result.className = `repair-result check-item status-${r.is_ok ? 'success' : 'warning'} fade-up`
      result.innerHTML = `
        <div class="check-icon">${r.is_ok ? '✅' : '⚠️'}</div>
        <div class="check-body"><div class="check-detail">${r.detail}</div></div>`
      btn.innerHTML = r.is_ok ? '✅ Réparation terminée' : '⚠️ Terminée avec avertissements'
    } catch (e) {
      result.className = 'repair-result check-item status-error fade-up'
      result.innerHTML = `<div class="check-icon">❌</div><div class="check-body"><div class="check-detail">Erreur : ${e}</div></div>`
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
      result.className = `defender-result check-item status-${r.is_ok ? 'success' : 'warning'} fade-up`
      result.innerHTML = `
        <div class="check-icon">${r.is_ok ? '✅' : '⚠️'}</div>
        <div class="check-body"><div class="check-detail">${r.detail}</div></div>`
      btn.innerHTML = r.is_ok ? '✅ Defender activé' : '⚠️ Activation avec avertissements'
    } catch (e) {
      result.className = 'defender-result check-item status-error fade-up'
      result.innerHTML = `<div class="check-icon">❌</div><div class="check-body"><div class="check-detail">Erreur : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = '🛡️ Réessayer'
    }
  })
}
