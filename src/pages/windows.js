import { invoke } from '@tauri-apps/api/core'
import { getOsInfo, getRamInfo } from 'tauri-plugin-hwinfo'

const MIN_BUILD = 26200
const MIN_RAM_MB = 16 * 1024

export function renderWindows(container, navigate) {
  container.innerHTML = `
    <button class="back-btn" id="back-btn">← Accueil</button>
    <div class="page-header">
      <h2>🪟 Windows &amp; Maintenance</h2>
      <p>Vérification de la version du système, intégrité des fichiers et santé du disque</p>
    </div>
    <button class="btn-launch" id="launch-btn">▶ Lancer la vérification</button>
    <div class="checks-list" id="checks-list"></div>
    <div class="repair-actions" id="repair-actions"></div>
  `

  container.querySelector('#back-btn').onclick = () => navigate('welcome')

  const launchBtn    = container.querySelector('#launch-btn')
  const checksList   = container.querySelector('#checks-list')
  const repairArea   = container.querySelector('#repair-actions')

  launchBtn.addEventListener('click', async () => {
    launchBtn.disabled = true
    launchBtn.innerHTML = '<span class="spinner"></span> Vérification en cours…'
    checksList.innerHTML = ''
    repairArea.innerHTML = ''

    let sfcError   = false
    let chkdskError = false

    // ── 1. Windows version ───────────────────────────────────────────────────
    const versionItem = addCheckItem(checksList, {
      icon: '⏳', label: 'Version de Windows',
      detail: 'Récupération des informations système…', status: 'running',
    })
    try {
      const osInfo = await getOsInfo()
      const parts = (osInfo.version || '').split('.')
      const build = parseInt(parts[2] ?? '0', 10)
      const isOk  = build >= MIN_BUILD
      setCheckItem(versionItem, {
        icon: isOk ? '✅' : '⚠️',
        label: 'Version de Windows',
        detail: isOk
          ? `${osInfo.name} (build ${build}) — Version conforme.`
          : `${osInfo.name} (build ${build}) — Windows 11 25H2 (build ${MIN_BUILD}) minimum requis. Veuillez mettre à jour via Windows Update.`,
        badge: isOk ? { text: 'Conforme', cls: 'badge-success' } : { text: 'Mise à jour requise', cls: 'badge-warning' },
        status: isOk ? 'success' : 'warning',
      })
    } catch (e) {
      setCheckItem(versionItem, { icon: '❌', label: 'Version de Windows', detail: 'Erreur : ' + e, badge: { text: 'Erreur', cls: 'badge-error' }, status: 'error' })
    }

    // ── 2. RAM ───────────────────────────────────────────────────────────────
    const ramItem = addCheckItem(checksList, {
      icon: '⏳', label: 'Mémoire vive (RAM)',
      detail: 'Récupération de la taille mémoire…', status: 'running',
    })
    try {
      const ramInfo = await getRamInfo()
      const sizeMb  = ramInfo.sizeMb ?? 0
      const sizeGb  = (sizeMb / 1024).toFixed(1)
      const isOk    = sizeMb >= MIN_RAM_MB
      setCheckItem(ramItem, {
        icon: isOk ? '✅' : '⚠️',
        label: 'Mémoire vive (RAM)',
        detail: isOk
          ? `${sizeGb} Go détectés — Capacité suffisante.`
          : `${sizeGb} Go détectés — Mémoire vive insuffisante. 16 Go minimum recommandés.`,
        badge: isOk ? { text: `${sizeGb} Go`, cls: 'badge-success' } : { text: 'Mémoire vive insuffisante', cls: 'badge-warning' },
        status: isOk ? 'success' : 'warning',
      })
    } catch (e) {
      setCheckItem(ramItem, { icon: '❌', label: 'Mémoire vive (RAM)', detail: 'Erreur : ' + e, badge: { text: 'Erreur', cls: 'badge-error' }, status: 'error' })
    }

    // ── 3. SFC ───────────────────────────────────────────────────────────────
    const sfcItem = addCheckItem(checksList, {
      icon: '⏳', label: 'Intégrité des fichiers système (SFC)',
      detail: 'Analyse en cours — peut prendre plusieurs minutes…', status: 'running',
    })
    try {
      const r = await invoke('run_sfc_check')
      sfcError = !r.is_ok
      setCheckItem(sfcItem, {
        icon: r.is_ok ? '✅' : '🔧',
        label: 'Intégrité des fichiers système (SFC)',
        detail: r.detail,
        badge: r.is_ok ? { text: 'OK', cls: 'badge-success' } : { text: 'Erreur détectée', cls: 'badge-warning' },
        status: r.is_ok ? 'success' : 'warning',
      })
    } catch (e) {
      sfcError = true
      setCheckItem(sfcItem, { icon: '❌', label: 'Intégrité des fichiers système (SFC)', detail: 'Erreur : ' + e, badge: { text: 'Erreur', cls: 'badge-error' }, status: 'error' })
    }

    // ── 4. CHKDSK ────────────────────────────────────────────────────────────
    const chkItem = addCheckItem(checksList, {
      icon: '⏳', label: 'Santé du disque C: (CHKDSK)',
      detail: 'Analyse du disque en cours…', status: 'running',
    })
    try {
      const r = await invoke('run_chkdsk')
      chkdskError = !r.is_ok
      setCheckItem(chkItem, {
        icon: r.is_ok ? '✅' : '🔧',
        label: 'Santé du disque C: (CHKDSK)',
        detail: r.detail,
        badge: r.is_ok ? { text: 'OK', cls: 'badge-success' } : { text: 'Erreur détectée', cls: 'badge-warning' },
        status: r.is_ok ? 'success' : 'warning',
      })
    } catch (e) {
      chkdskError = true
      setCheckItem(chkItem, { icon: '❌', label: 'Santé du disque C: (CHKDSK)', detail: 'Erreur : ' + e, badge: { text: 'Erreur', cls: 'badge-error' }, status: 'error' })
    }

    // ── Done — show repair buttons if needed, then home ───────────────────────
    launchBtn.disabled = false
    launchBtn.innerHTML = '🔄 Relancer la vérification'

    if (sfcError) {
      addRepairButton(repairArea, {
        id:    'btn-repair-sfc',
        icon:  '🛠️',
        label: 'Réparer Windows',
        detail: 'Exécute : dism /online /cleanup-image /restorehealth',
        kind:  'sfc',
      })
    }

    if (chkdskError) {
      addRepairButton(repairArea, {
        id:    'btn-repair-chkdsk',
        icon:  '💾',
        label: 'Réparer le disque',
        detail: 'Exécute : chkdsk C: /f /r',
        kind:  'chkdsk',
      })
    }

    const homeBtn = document.createElement('button')
    homeBtn.className = 'btn-home fade-up'
    homeBtn.innerHTML = '🏠 Retour à l\'accueil'
    homeBtn.onclick = () => navigate('welcome')
    repairArea.appendChild(homeBtn)
  })
}

// ── Repair button helper ──────────────────────────────────────────────────────
function addRepairButton(area, { id, icon, label, detail, kind }) {
  const wrapper = document.createElement('div')
  wrapper.className = 'repair-block fade-up'
  wrapper.id = id + '-wrapper'
  wrapper.innerHTML = `
    <div class="repair-info">
      <span class="repair-icon">${icon}</span>
      <div>
        <div class="repair-label">${label}</div>
        <div class="repair-detail">${detail}</div>
      </div>
    </div>
    <button class="btn-repair" id="${id}">${icon} ${label}</button>
    <div class="repair-result hidden" id="${id}-result"></div>
  `
  area.appendChild(wrapper)

  wrapper.querySelector(`#${id}`).addEventListener('click', async () => {
    const btn    = wrapper.querySelector(`#${id}`)
    const result = wrapper.querySelector(`#${id}-result`)

    btn.disabled = true
    btn.innerHTML = '<span class="spinner"></span> Réparation en cours…'
    result.className = 'repair-result check-item status-running fade-up'
    result.innerHTML = `<div class="check-icon">⏳</div><div class="check-body"><div class="check-detail">Exécution en cours, veuillez patienter…</div></div>`

    try {
      const r = await invoke('run_repair', { kind })
      result.className = `repair-result check-item status-${r.is_ok ? 'success' : 'warning'} fade-up`
      result.innerHTML = `
        <div class="check-icon">${r.is_ok ? '✅' : '⚠️'}</div>
        <div class="check-body"><div class="check-detail">${r.detail}</div></div>
      `
      btn.innerHTML = r.is_ok ? '✅ Réparation terminée' : '⚠️ Réparation terminée avec avertissements'
    } catch (e) {
      result.className = 'repair-result check-item status-error fade-up'
      result.innerHTML = `<div class="check-icon">❌</div><div class="check-body"><div class="check-detail">Erreur : ${e}</div></div>`
      btn.disabled = false
      btn.innerHTML = `${icon} Réessayer`
    }
  })
}

// ── Check item helpers ────────────────────────────────────────────────────────
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