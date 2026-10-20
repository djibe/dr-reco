# Dr Reco

Dr Reco est un logiciel qui vise à assister le médecin généraliste dans l'entretien de son système Microsoft Windows (intégrité du système, stockage ...) ainsi que dans la gestion des paquets Amelipro (cryptolib, composants Assurance maladie, plugin navigateur).

## ⏬ Télécharger la dernière version: [Dr Reco en dernière version](https://github.com/djibe/dr-reco/releases/latest/download/dr-reco.exe)

## Verifications Windows

- Point de restauration système
- Version de Windows (Windows 11 26H2)
- Taille de la mémoire vive (RAM)
- Type de stockage (SSD)
- Environnement de récupération Windows (WinRE)
- Vérification des fichiers Windows (sfc, DISM)
- Nettoyage du disque
- Santé du système de fichiers (chkdsk)
- Antivirus activé
- Démarrage rapide de Windows activé
- Ordinateur portable: santé de la batterie
- Réparation automatique de Windows activée (QMR)
- Nettoyage des anciens composants Windows (DISM)
- Compression du système (CompactOS)

## Verifications Ameli

- Version de Cryptolib et mise à jour
- Version des services CNAM et mise à jour
- Présence de lecteur de carte à puce
- Service Carte à puce (SCardSvr) : démarrage automatique et service démarré (réparation en 1 clic)
- Version du navigateur
- Extension navigateur pour la lecture de la carte vitale
- Éviter les déconnexions du lecteur CPS  
  Désactivation du délai de mise en veille des hubs USB, de la mise en veille sélective USB et de la gestion de l'alimentation de la liaison USB 3.

<img width="922" height="732" alt="image" src="https://github.com/user-attachments/assets/285359b3-67e6-40b6-ae9c-8e6f7a1812a8">

## Développer

- Installer Git, NPM (ex. [NVM Windows](https://nvm-windows.com/)), [Microsoft C++ Build Tools](https://aka.ms/vs/stable/vs_BuildTools.exe) et [Rust](https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe) (chaîne d'outils **MSVC**)
- `git clone https://github.com/djibe/dr-reco.git`
- `cd dr-reco`
- `npm install`
- Build: `npm run tauri:build` (charge l'environnement Visual Studio et produit un exécutable autonome)
