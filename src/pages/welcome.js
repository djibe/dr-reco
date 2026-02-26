export function renderWelcome(container, navigate) {
  container.innerHTML = `
    <div class="welcome-header fade-up">
      <h1>Dr Reco</h1>
      <p>Votre assistant de diagnostic et de maintenance informatique pour cabinets médicaux</p>
    </div>
    <div class="cards-grid">
      <div class="nav-card fade-up fade-up-delay-1" data-target="windows">
        <div class="nav-card-icon">🪟</div>
        <h3>Windows &amp; Maintenance</h3>
        <p>Vérifiez l'état de votre système Windows : version, intégrité des fichiers système et santé du disque.</p>
        <div class="nav-card-arrow">→</div>
      </div>
      <div class="nav-card fade-up fade-up-delay-2" data-target="amelipro">
        <div class="nav-card-icon">🏥</div>
        <h3>Outils Amelipro</h3>
        <p>Diagnostiquez et vérifiez les prérequis logiciels nécessaires à l'utilisation d'Amelipro.</p>
        <div class="nav-card-arrow">→</div>
      </div>
      <div class="nav-card fade-up fade-up-delay-3" data-target="about">
        <div class="nav-card-icon">ℹ️</div>
        <h3>À propos</h3>
        <p>Informations sur Dr Reco, son auteur et le code source du projet sur GitHub.</p>
        <div class="nav-card-arrow">→</div>
      </div>
    </div>
  `

  container.querySelectorAll('.nav-card').forEach(card => {
    card.addEventListener('click', () => {
      navigate(card.dataset.target)
    })
  })
}
