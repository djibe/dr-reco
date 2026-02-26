import { renderWelcome }  from './pages/welcome.js'
import { renderWindows }  from './pages/windows.js'
import { renderAmelipro } from './pages/amelipro.js'
import { renderAbout }    from './pages/about.js'

let currentPage = 'welcome'

export function navigate(page) {
  currentPage = page
  render()
}

function render() {
  const app = document.getElementById('app')
  app.innerHTML = ''

  // ── Topbar ──────────────────────────────────────────────────────────────────
  const topbar = document.createElement('div')
  topbar.className = 'topbar'

  const brand = document.createElement('button')
  brand.className = 'topbar-brand'
  brand.innerHTML = '🩺 Dr Reco'
  brand.onclick = () => navigate('welcome')
  topbar.appendChild(brand)

  const pageTitles = {
    windows:  'Windows & Maintenance',
    amelipro: 'Outils Amelipro',
    about:    'À propos',
  }

  if (currentPage !== 'welcome') {
    const sep = document.createElement('span')
    sep.className = 'topbar-sep'
    sep.textContent = '/'
    topbar.appendChild(sep)

    const title = document.createElement('span')
    title.className = 'topbar-title'
    title.textContent = pageTitles[currentPage] || ''
    topbar.appendChild(title)
  }

  app.appendChild(topbar)

  // ── Main ────────────────────────────────────────────────────────────────────
  const main = document.createElement('main')
  main.className = 'main-content'

  switch (currentPage) {
    case 'welcome':  renderWelcome(main, navigate);  break
    case 'windows':  renderWindows(main, navigate);  break
    case 'amelipro': renderAmelipro(main, navigate); break
    case 'about':    renderAbout(main, navigate);    break
  }

  app.appendChild(main)
}

export function renderApp() { render() }
