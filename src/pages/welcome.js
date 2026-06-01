export function renderWelcome(container, navigate) {
  container.innerHTML = `
    <div class="dr-welcome-header fade-up">
      <h1>🩺 Dr Reco</h1>
      <p>Assistant de diagnostic et maintenance informatique pour cabinets médicaux</p>
    </div>

    <div class="grid-3">
      <div>
        <button class="dr-nav-card fade-up delay-1" data-target="windows">
          <div class="dr-nav-card-icon">
          <svg xmlns="http://www.w3.org/2000/svg" width="24" heigh="24" fill-rule="evenodd" clip-rule="evenodd" image-rendering="optimizeQuality" shape-rendering="geometricPrecision" text-rendering="geometricPrecision" viewBox="0 0 512 512.02">
            <path fill="var(--dr-primary)" fill-rule="nonzero" d="M0 512.02h242.686V269.335H0zm0-269.334h242.686V0H0zm269.314 0H512V0H269.314zm0 269.334H512V269.335H269.314z"/>
          </svg>
          </div>
          <h3>Windows &amp; Maintenance</h3>
          <p>Vérifiez la version Windows, l'intégrité des fichiers système et la santé du disque.</p>
          <div class="dr-nav-card-arrow">→</div>
        </button>
      </div>

      <div>
        <button class="dr-nav-card fade-up delay-2" data-target="amelipro">
          <div class="dr-nav-card-icon">
            <svg xmlns="http://www.w3.org/2000/svg" height="24px" viewBox="0 -960 960 960" width="24px" fill="var(--dr-primary)"><path d="M420-360h120l-23-129q20-10 31.5-29t11.5-42q0-33-23.5-56.5T480-640q-33 0-56.5 23.5T400-560q0 23 11.5 42t31.5 29l-23 129Zm60 280q-139-35-229.5-159.5T160-516v-244l320-120 320 120v244q0 152-90.5 276.5T480-80Zm0-84q104-33 172-132t68-220v-189l-240-90-240 90v189q0 121 68 220t172 132Zm0-316Z"/></svg>
          </div>
          <h3>Outils Amelipro</h3>
          <p>Diagnostiquez les pré-requis nécessaires à l'utilisation d'Amelipro et de la carte CPS.</p>
          <div class="dr-nav-card-arrow">→</div>
        </button>
      </div>

      <div>
        <button class="dr-nav-card fade-up delay-3" data-target="about">
          <div class="dr-nav-card-icon">
            <svg xmlns="http://www.w3.org/2000/svg" height="24px" viewBox="0 -960 960 960" width="24px" fill="var(--dr-primary)"><path d="M440-280h80v-240h-80v240Zm68.5-331.5Q520-623 520-640t-11.5-28.5Q497-680 480-680t-28.5 11.5Q440-657 440-640t11.5 28.5Q463-600 480-600t28.5-11.5ZM480-80q-83 0-156-31.5T197-197q-54-54-85.5-127T80-480q0-83 31.5-156T197-763q54-54 127-85.5T480-880q83 0 156 31.5T763-763q54 54 85.5 127T880-480q0 83-31.5 156T763-197q-54 54-127 85.5T480-80Zm0-80q134 0 227-93t93-227q0-134-93-227t-227-93q-134 0-227 93t-93 227q0 134 93 227t227 93Zm0-320Z"/></svg>
          </div>
          <h3>À propos</h3>
          <p>Informations sur Dr Reco, son auteur et le code source.</p>
          <div class="dr-nav-card-arrow">→</div>
        </button>
      </div>
    </div>
  `

  container.querySelectorAll('.dr-nav-card').forEach(card => {
    card.addEventListener('click', () => navigate(card.dataset.target))
  })
}
