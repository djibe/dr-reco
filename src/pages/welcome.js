export function renderWelcome(container, navigate) {
  container.innerHTML = `
    <div class="welcome-header fade-up">
      <fluent-text block as="h1" size="800" weight="bold"><span>🩺 Dr Reco</span></fluent-text>

      <fluent-text as="h4" style="margin-top: "><span>Assistant de diagnostic et maintenance informatique pour cabinets médicaux</span></fluent-text>
    </div>

    <div class="cards-grid">
      <div class="nav-card fade-up delay-1" data-target="windows">
        <div class="nav-card-icon">🪟</div>
        <h3>Windows &amp; Maintenance</h3>
        <fluent-divider></fluent-divider>
        <p>Vérifiez la version Windows, l'intégrité des fichiers système et la santé du disque.</p>
        <div class="nav-card-arrow">→</div>
      </div>

      <div class="nav-card fade-up delay-2" data-target="amelipro">
        <div class="nav-card-icon">🏥</div>
        <h3>Outils Amelipro</h3>
        <fluent-divider></fluent-divider>
        <p>Diagnostiquez les prérequis logiciels nécessaires à l'utilisation d'Amelipro.</p>
        <div class="nav-card-arrow">→</div>
      </div>

      <div class="nav-card fade-up delay-3" data-target="about">
        <div class="nav-card-icon">ℹ️</div>
        <h3>À propos</h3>
        <fluent-divider></fluent-divider>
        <p>Informations sur Dr Reco, son auteur et le code source sur GitHub.</p>
        <div class="nav-card-arrow">→</div>
      </div>
    </div>
  `

  container.querySelectorAll('.nav-card').forEach(card => {
    card.addEventListener('click', () => navigate(card.dataset.target))
  })
}
