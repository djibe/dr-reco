import { renderWelcome }  from './pages/welcome.js'
import { renderWindows }  from './pages/windows.js'
import { renderAmelipro } from './pages/amelipro.js'
import { renderAbout }    from './pages/about.js'

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
}

export function renderApp() { render() }