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
