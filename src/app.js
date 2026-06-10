import { renderWelcome }  from './pages/welcome.js'
import { renderWindows }  from './pages/windows.js'
import { renderAmelipro } from './pages/amelipro.js'
import { renderAbout }    from './pages/about.js'
import { getVersion }     from '@tauri-apps/api/app'
import { invoke }         from '@tauri-apps/api/core'

const GITHUB_REPO = 'djibe/dr-reco'
const RELEASES_API = `https://api.github.com/repos/${GITHUB_REPO}/releases/latest`

let currentPage = 'welcome'

export function navigate(page) {
  currentPage = page
  render()
}

const pageTitles = {
  windows:  'Windows & Maintenance',
  amelipro: 'Outils Amelipro',
  about:    'À propos',
}

function render() {
  const app = document.getElementById('app')
  app.innerHTML = ''

  // ── Navbar ───────────────────────────────────────────────────────────────────
  const navbar = document.createElement('nav')
  navbar.className = 'dr-navbar'

  const brand = document.createElement('button')
  brand.className = 'dr-navbar-brand'
  brand.innerHTML = '🩺 Dr Reco'
  brand.onclick = () => navigate('welcome')
  navbar.appendChild(brand)

  if (currentPage !== 'welcome') {
    const sep = document.createElement('span')
    sep.className = 'dr-navbar-sep'
    sep.textContent = '/'
    navbar.appendChild(sep)

    const title = document.createElement('span')
    title.className = 'dr-navbar-title'
    title.textContent = pageTitles[currentPage] || ''
    navbar.appendChild(title)
  }

  app.appendChild(navbar)

  // ── Main ─────────────────────────────────────────────────────────────────────
  const main = document.createElement('main')
  main.className = 'dr-main'

  switch (currentPage) {
    case 'welcome':  renderWelcome(main, navigate);  break
    case 'windows':  renderWindows(main, navigate);  break
    case 'amelipro': renderAmelipro(main, navigate); break
    case 'about':    renderAbout(main, navigate);    break
  }

  app.appendChild(main)

  // ── Footer ───────────────────────────────────────────────────────────────────
  const footer = document.createElement('footer')
  footer.className = 'dr-footer'

  const versionBtn = document.createElement('button')
  versionBtn.className = 'dr-footer-version'
  versionBtn.textContent = 'v…'
  versionBtn.title = 'Vérifier les mises à jour'
  versionBtn.addEventListener('click', () => openUpdateModal())
  footer.appendChild(versionBtn)
  getVersion().then(v => { versionBtn.textContent = `v${v}` }).catch(() => {})

  app.appendChild(footer)
}

// ── Update modal ──────────────────────────────────────────────────────────────

async function openUpdateModal() {
  const version = await getVersion().catch(() => '?')

  // Backdrop
  const backdrop = document.createElement('div')
  backdrop.className = 'dr-modal-backdrop'
  backdrop.addEventListener('click', e => { if (e.target === backdrop) backdrop.remove() })

  const modal = document.createElement('div')
  modal.className = 'dr-modal'
  modal.setAttribute('role', 'dialog')
  modal.setAttribute('aria-modal', 'true')

  // Close button
  const closeBtn = document.createElement('button')
  closeBtn.className = 'dr-modal-close'
  closeBtn.innerHTML = '✕'
  closeBtn.setAttribute('aria-label', 'Fermer')
  closeBtn.addEventListener('click', () => backdrop.remove())
  modal.appendChild(closeBtn)

  // Initial loading state
  modal.innerHTML += `
    <h3>🔄 Vérification des mises à jour</h3>
    <div class="dr-modal-meta">
      Version installée :
      <span class="dr-version-chip dr-version-current">v${version}</span>
    </div>
    <div class="dr-modal-body" id="modal-body">
      <div style="display:flex;align-items:center;gap:.6rem;color:var(--dr-on-surface-variant)">
        <span class="dr-spinner"></span> Récupération de la dernière version…
      </div>
    </div>
    <div class="dr-modal-actions" id="modal-actions"></div>
  `

  backdrop.appendChild(modal)
  document.body.appendChild(backdrop)

  // Escape key
  const onKey = e => { if (e.key === 'Escape') { backdrop.remove(); document.removeEventListener('keydown', onKey) } }
  document.addEventListener('keydown', onKey)

  fetchLatestRelease(modal, version)
}

async function fetchLatestRelease(modal, version) {
  const bodyEl   = modal.querySelector('#modal-body')
  const actionsEl = modal.querySelector('#modal-actions')

  try {
    const res = await fetch(RELEASES_API, {
      headers: { 'Accept': 'application/vnd.github+json' }
    })

    if (!res.ok) throw new Error(`GitHub API: ${res.status}`)

    const release = await res.json()
    const tag     = release.tag_name || ''                       // e.g. "v1.2.0"
    const latest  = tag.replace(/^v/, '')                        // "1.2.0"
    const isNewer = compareVersions(latest, version) > 0

    // Update modal title
    modal.querySelector('h3').textContent = isNewer ? '🆕 Nouvelle version disponible' : '✅ Dr Reco est à jour'

    // Version chips
    modal.querySelector('.dr-modal-meta').innerHTML = `
      Version installée : <span class="dr-version-chip dr-version-current">v${version}</span>
      &nbsp;
      Dernière version : <span class="dr-version-chip ${isNewer ? 'dr-version-new' : 'dr-version-same'}">${tag}</span>
    `

    // Release notes (strip markdown headings, limit length)
    const notes = (release.body || 'Aucune note de version disponible.')
      .replace(/#{1,6}\s*/g, '')
      .replace(/\*\*/g, '')
      .trim()
      .split('\n')
      .slice(0, 12)
      .join('\n')

    bodyEl.innerHTML = notes
      .split('\n')
      .filter(l => l.trim())
      .map(l => `<p>${escHtml(l)}</p>`)
      .join('')

    // Actions
    if (isNewer) {
      // Find the Windows installer asset
      const assets  = release.assets || []
      const installer = assets.find(a =>
        /\.(exe|msi)$/i.test(a.name) && /windows|win|setup|install/i.test(a.name)
      ) || assets.find(a => /\.(exe|msi)$/i.test(a.name))

      const downloadUrl = installer ? installer.browser_download_url : release.html_url

      const dlBtn = document.createElement('button')
      dlBtn.className = 'btn-dr-primary'
      dlBtn.innerHTML = `⬇️ Télécharger ${tag}`
      dlBtn.addEventListener('click', async () => {
        dlBtn.disabled = true
        dlBtn.innerHTML = '<span class="dr-spinner"></span> Ouverture…'
        try {
          await invoke('open_url', { url: downloadUrl })
          dlBtn.innerHTML = '✅ Téléchargement ouvert'
        } catch {
          dlBtn.disabled = false
          dlBtn.innerHTML = `⬇️ Télécharger ${tag}`
        }
      })
      actionsEl.appendChild(dlBtn)
    }

    const githubBtn = document.createElement('button')
    githubBtn.className = 'btn-dr-secondary'
    githubBtn.innerHTML = 'Voir sur GitHub'
    githubBtn.addEventListener('click', () => invoke('open_url', { url: release.html_url }))
    actionsEl.appendChild(githubBtn)

  } catch (e) {
    bodyEl.innerHTML = `
      <p style="color:var(--dr-danger)">
        Impossible de contacter GitHub. Vérifiez votre connexion internet.
      </p>
      <p style="font-size:.75rem;color:var(--dr-on-surface-variant)">${escHtml(String(e))}</p>
    `
    const githubBtn = document.createElement('button')
    githubBtn.className = 'btn-dr-secondary'
    githubBtn.innerHTML = 'Ouvrir GitHub'
    githubBtn.addEventListener('click', () => invoke('open_url', { url: `https://github.com/${GITHUB_REPO}/releases` }))
    actionsEl.appendChild(githubBtn)
  }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function compareVersions(a, b) {
  const pa = a.split('.').map(Number)
  const pb = b.split('.').map(Number)
  const len = Math.max(pa.length, pb.length)
  for (let i = 0; i < len; i++) {
    const x = pa[i] ?? 0
    const y = pb[i] ?? 0
    if (x > y) return  1
    if (x < y) return -1
  }
  return 0
}

function escHtml(str) {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

export function renderApp() { render() }
