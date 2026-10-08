# AGENTS.md — Dr Reco

> Reference document for AI agents (and humans) contributing to this project.
> This file is written in English. Language rule: **all user-visible UI strings
> must be written in French, but all code stays in English** (identifiers,
> function names, comments, commit messages).

## IMPORTANT

NEVER trigger any CI/CD pipeline automatically.

## 1. Overview

**Dr Reco** helps general practitioners maintain their Windows workstation
(system integrity, storage, recovery…) and manage Amelipro components
(Cryptolib, CNAM services, smart card reader, browser).

- **Windows-only** desktop application, built with **Tauri 2**.
- UI in French, aimed at non-technical users: plain-language messages,
  one-click repair actions.
- Repository: `https://github.com/djibe/dr-reco` — license `CC-BY-NC-SA-4.0`.

## 2. Stack & prerequisites

| Layer    | Tech                                            |
|----------|-------------------------------------------------|
| Frontend | Vanilla JS (ES modules, **no framework**), Vite 8, custom CSS (`src/style.css`) |
| Backend  | Rust (edition 2024), Tauri 2, `winreg`, `tauri-plugin-shell/hwinfo/notification` |
| Target   | Windows 10/11 (diagnostics only make sense on Windows) |

Prerequisites: **Node `>=22 <25`**, **Rust ≥ 1.93** toolchain (`rustc`, `cargo`),
Tauri CLI (via `npm run tauri`). The backend can only be tested on Windows;
on Linux/macOS, only the Vite frontend is verifiable.

## 3. Commands

```powershell
npm run dev            # Frontend dev only (http://localhost:1420)
npm run build          # Build frontend -> dist/
npm run preview        # Preview the frontend build
npm run tauri          # Tauri CLI (e.g. npm run tauri dev)
npm run tauri:build    # Full installer / .exe build
npm run tauri:update   # cargo update inside src-tauri/
npm run generate:icons # Regenerate icons from public/favicon.svg
```

```powershell
cargo update           # Update crates (from src-tauri/)
```

No test framework: **no `test` script**. Verify with `npm run build`
and `cargo check` / `cargo clippy` (from `src-tauri/`).

## 4. Architecture

```text
src/
  main.js        # entry point -> renderApp()
  app.js         # minimal router (navigate), navbar, footer, GitHub update modal
  notify.js      # notification helper
  style.css      # all styling (dr- and btn-dr-* prefixed classes)
  pages/
    welcome.js   # home page
    windows.js   # Windows diagnostics & maintenance
    amelipro.js  # Ameli diagnostics (Cryptolib, CNAM, reader, browser, USB)
    about.js     # about page

src-tauri/
  src/main.rs               # ALL backend code: helpers + ~27 Tauri commands
  tauri.conf.json           # app config (920×700 window, Dark theme, bundle)
  capabilities/default.json # Tauri permissions (core, hwinfo, shell, notification)
  Cargo.toml / Cargo.lock   # Rust dependencies
```

Typical flow: JS page → `invoke('<command>')` (`@tauri-apps/api/core`)
→ Rust command in `main.rs` → typed JSON response → DOM rendering.

## 5. Backend conventions (Rust, `src-tauri/src/main.rs`)

- **Standard contract** — return `CheckResult`:
  ```rust
  struct CheckResult { is_ok: bool, detail: String, not_found: bool, ps_unavailable: bool }
  ```
  Constructors: `ok()`, `err()`, `unavailable()` (PowerShell unreachable),
  `missing()` (component absent). Specific results: `AntivirusResult`,
  `BatteryResult`, `BrowserResult`, `BrowserVersionResult`.
- **PowerShell execution**: always go through the existing helpers —
  `powershell()` (PS scripts, forced UTF-8 prefix) or `powershell_native()`
  (native binaries like `sfc`, `chkdsk`, `reagentc`: wrapped via `cmd.exe` + a
  temp file to re-encode OEM output as UTF-8). Never call `powershell.exe` directly.
- **Registry**: via `winreg`, checking both `HKLM\SOFTWARE\…` and
  `HKLM\SOFTWARE\WOW6432Node\…` views for 32-bit components.
- **Mandatory registration**: every new `#[tauri::command]` must be added to
  `tauri::generate_handler![…]` in `fn main()`.
- `detail` messages are **user-facing: write them in French**, plain language
  (no jargon), stating the expected action when relevant.
- Compare versions with the existing `compare_versions()` helper, never by hand.
- **Code language**: identifiers, function names, and code comments in English.
  Only user-facing strings are in French.

## 6. Frontend conventions (JS, `src/`)

- **No framework**: vanilla DOM (`createElement` / `innerHTML` + `querySelector`).
- Pages: export `renderX(container, navigate)` from `src/pages/*.js`.
  Do not introduce React/Vue/Svelte without prior discussion.
- Backend: `import { invoke } from '@tauri-apps/api/core'`, then
  `await invoke('command_name', { args })`. Handle `ps_unavailable` (warning)
  and user cancellation (the `cancelled` pattern in `windows.js`).
- Hardware info: `getOsInfo()` / `getRamInfo()` from `tauri-plugin-hwinfo`
  (current thresholds: build ≥ 26300, RAM ≥ 15 GB — see `windows.js`).
- Notifications via `src/notify.js`. Styles: reuse existing classes
  (`dr-*`, `btn-dr-primary/secondary/subtle`) from `src/style.css`.
- **All visible strings in French**. Escape injected HTML (see `escHtml` in `app.js`).
- `tauri.conf.json` sets `csp: null`: any external opening must go through the
  `open_url` command, never via direct `window.open`.

## 7. Versions & release

- **Keep the 3 files in sync** on every release: `package.json`,
  `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` (currently `0.6.2`).
- Windows metadata (name, copyright, `CompanyName`) lives in
  `[package.metadata.tauri-winres]` in `Cargo.toml`.
- Minimal permissions in `src-tauri/capabilities/default.json`: only add a
  permission if the command genuinely requires it.
- Ignored build artifacts: `node_modules/`, `dist/`, `src-tauri/target/`,
  `todo.md` (local roadmap, **git-ignored** — do not commit it).

## 8. Adding a new diagnostic (recipe)

1. **Rust** (`src-tauri/src/main.rs`): write `#[tauri::command] async fn …`
   returning `CheckResult` (or a dedicated serializable struct), using
   `powershell()` / `powershell_native()` / `winreg`. Code and comments in English,
   user-facing `detail` strings in French.
2. **Register** the command in `generate_handler![…]` in `fn main()`.
3. **Permission**: check whether an extra entry is needed in
   `capabilities/default.json`.
4. **Frontend** (`src/pages/windows.js` or `amelipro.js`): `invoke('…')`,
   render the result with the existing card helpers (`addCheck`/`setCheck`),
   offer a repair button when applicable (button label in French).
5. **Verify**: `npm run build` + `cargo check` (in `src-tauri/`), then manual
   test on Windows (ideally with and without admin rights).

## 9. Pitfalls / Don'ts

- **Mangled accents**: known issue (see `todo.md`). Never parse raw output of
  Windows binaries: always go through `powershell_native()`.
- **Admin rights**: many commands fail without elevation — return
  `unavailable`/`err` with an explicit message, never panic.
- **Windows-only**: `Get-CimInstance`, `reagentc`, `HKLM` keys… do not work
  elsewhere. Do not "fix" things by breaking Windows compatibility.
- **Hardcoded versions**: Cryptolib min `5.2.6`, `SrvSvCnam 5.10.04`, browser
  majors (Chrome 153 / Firefox 155 / Edge 152) — update periodically, flag any
  stale value instead of letting it rot.
- Do not add npm/crate dependencies without real need (maintenance surface + Windows bundle).
- Do not commit `todo.md`, `dist/`, `target/`, `node_modules/`.

## 10. Current state & roadmap

- The working roadmap lives in **`todo.md` (local, unversioned)**: check it at
  the start of a session for context (e.g. SCardSvr, USB power management,
  package signing, update page).
- Before a session: read `README.md` (functional scope), `todo.md`
  (work in progress) and `git log --oneline -10` (recent history).
