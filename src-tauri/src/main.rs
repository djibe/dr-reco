#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::process::Command;
use std::os::windows::process::CommandExt;
use winreg::enums::*;
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x08000000;

// ─── Shared types ─────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct CheckResult {
    is_ok: bool,
    detail: String,
    /// true  → PowerShell could not be launched (binary missing / permission denied)
    /// reused for "not installed" in registry checks
    #[serde(default)]
    not_found: bool,
    /// true → command ran but PowerShell itself failed to spawn
    #[serde(default)]
    ps_unavailable: bool,
}

impl CheckResult {
    fn ok(detail: impl Into<String>) -> Self {
        Self { is_ok: true, detail: detail.into(), not_found: false, ps_unavailable: false }
    }
    fn err(detail: impl Into<String>) -> Self {
        Self { is_ok: false, detail: detail.into(), not_found: false, ps_unavailable: false }
    }
    fn unavailable(detail: impl Into<String>) -> Self {
        Self { is_ok: false, detail: detail.into(), not_found: false, ps_unavailable: true }
    }
    fn missing(detail: impl Into<String>) -> Self {
        Self { is_ok: false, detail: detail.into(), not_found: true, ps_unavailable: false }
    }
}

// ─── PowerShell helper ────────────────────────────────────────────────────────

enum PsResult {
    /// PowerShell ran; contains (stdout+stderr, exit_code)
    Ok(String, i32),
    /// powershell.exe could not be spawned at all
    SpawnFailed(String),
}

fn powershell(script: &str) -> PsResult {
    match Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NonInteractive",
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-Command", script,
        ])
        .output()
    {
        Err(e) => PsResult::SpawnFailed(format!(
            "PowerShell est introuvable ou inaccessible : {}. Vérifiez que PowerShell est installé sur ce système.",
            e
        )),
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            PsResult::Ok(format!("{}{}", stdout, stderr), output.status.code().unwrap_or(-1))
        }
    }
}

// ─── SFC /scannow ─────────────────────────────────────────────────────────────
#[tauri::command]
fn run_sfc_check() -> Result<CheckResult, String> {
    let (combined, _) = match powershell("sfc /scannow") {
        PsResult::SpawnFailed(e) => return Ok(CheckResult::unavailable(
            format!("Impossible d'exécuter SFC — {}. La vérification des fichiers système n'a pas pu être lancée.", e)
        )),
        PsResult::Ok(out, code) => (out, code),
    };
    let lower = combined.to_lowercase();

    let found_violations = lower.contains("found corrupt")
        || lower.contains("could not repair")
        || lower.contains("a trouvé des violations")
        || lower.contains("n'a pas pu les réparer");

    let repaired = lower.contains("successfully repaired")
        || lower.contains("réparé avec succès");

    let no_violations = lower.contains("did not find any integrity violations")
        || lower.contains("n'a trouvé aucune violation")
        || lower.contains("no integrity violations");

    if no_violations || repaired {
        Ok(CheckResult::ok("Aucune violation d'intégrité trouvée. Le système de fichiers est sain."))
    } else if found_violations {
        let _ = powershell("Start-Process -FilePath 'dism.exe' -ArgumentList '/Online','/Cleanup-Image','/RestoreHealth' -WindowStyle Hidden");
        Ok(CheckResult::err("Des violations d'intégrité ont été détectées. Une réparation DISM /RestoreHealth a été lancée. Redémarrez l'ordinateur pour appliquer les corrections."))
    } else if combined.trim().is_empty() {
        Ok(CheckResult::unavailable("SFC n'a produit aucun résultat. Des droits administrateur sont peut-être nécessaires."))
    } else {
        let preview: String = combined.chars().take(300).collect();
        Ok(CheckResult::ok(format!("SFC terminé. Résultat : {}", preview.trim())))
    }
}

// ─── CHKDSK C: ───────────────────────────────────────────────────────────────
#[tauri::command]
fn run_chkdsk() -> Result<CheckResult, String> {
    let (combined, exit_code) = match powershell("chkdsk C:") {
        PsResult::SpawnFailed(e) => return Ok(CheckResult::unavailable(
            format!("Impossible d'exécuter CHKDSK — {}. La vérification du disque n'a pas pu être lancée.", e)
        )),
        PsResult::Ok(out, code) => (out, code),
    };
    let lower = combined.to_lowercase();

    let has_errors = lower.contains("found errors")
        || lower.contains("errors found")
        || lower.contains("erreurs trouvées")
        || exit_code == 2
        || exit_code == 3;

    let already_scheduled = lower.contains("scheduled")
        || lower.contains("planifié")
        || lower.contains("cannot run");

    if has_errors && !already_scheduled {
        let _ = powershell("'Y' | chkdsk C: /f /r /x");
        Ok(CheckResult::err("Des erreurs ont été trouvées sur le disque C:. Une vérification complète (chkdsk /f /r) a été planifiée au prochain démarrage."))
    } else if already_scheduled {
        Ok(CheckResult::err("CHKDSK est déjà planifié au prochain démarrage. Redémarrez l'ordinateur pour lancer la vérification."))
    } else if exit_code == 0 || lower.contains("no problems found") || lower.contains("aucun problème") {
        Ok(CheckResult::ok("Le disque C: a été vérifié. Aucun problème détecté."))
    } else {
        Ok(CheckResult::ok(format!("CHKDSK terminé (code {}). Disque en bon état.", exit_code)))
    }
}

// ─── Cryptolib CPS ───────────────────────────────────────────────────────────
const MIN_CRYPTOLIB_VERSION: &str = "5.2.6";

#[tauri::command]
fn check_cryptolib_version() -> Result<CheckResult, String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    let key_result = hklm
        .open_subkey("SOFTWARE\\ASIP\\Cryptolib CPS")
        .or_else(|_| hklm.open_subkey("SOFTWARE\\WOW6432Node\\ASIP\\Cryptolib CPS"));

    match key_result {
        Ok(key) => match key.get_value::<String, _>("Version") {
            Ok(version) => {
                let version = version.trim().to_string();
                let is_ok = compare_versions(&version, MIN_CRYPTOLIB_VERSION) >= 0;
                let detail = if is_ok {
                    format!("Cryptolib CPS version {} — conforme (minimum requis : {}).", version, MIN_CRYPTOLIB_VERSION)
                } else {
                    format!("Cryptolib CPS version {} installée, mais {} minimum requis. Téléchargez la dernière version sur le site de l'ANS.", version, MIN_CRYPTOLIB_VERSION)
                };
                Ok(CheckResult { is_ok, detail, not_found: false, ps_unavailable: false })
            }
            Err(_) => Ok(CheckResult::missing("Cryptolib CPS trouvé dans le registre mais la valeur 'Version' est manquante.")),
        },
        Err(_) => Ok(CheckResult::missing("Cryptolib CPS n'est pas installé. Téléchargez-le depuis le portail de l'ANS (Agence du Numérique en Santé).")),
    }
}

fn compare_versions(a: &str, b: &str) -> i32 {
    let parse = |s: &str| -> Vec<u32> { s.split('.').filter_map(|x| x.parse().ok()).collect() };
    let va = parse(a);
    let vb = parse(b);
    let len = va.len().max(vb.len());
    for i in 0..len {
        let x = va.get(i).copied().unwrap_or(0);
        let y = vb.get(i).copied().unwrap_or(0);
        if x < y { return -1; }
        if x > y { return 1; }
    }
    0
}

// ─── Open URL ─────────────────────────────────────────────────────────────────
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    match powershell(&format!("Start-Process '{}'", url)) {
        PsResult::SpawnFailed(e) => Err(e),
        PsResult::Ok(_, _) => Ok(()),
    }
}

// ─── Repair commands ──────────────────────────────────────────────────────────
#[tauri::command]
fn run_repair(kind: String) -> Result<CheckResult, String> {
    let (script, label) = match kind.as_str() {
        "sfc"    => ("dism /online /cleanup-image /restorehealth", "Réparation DISM"),
        "chkdsk" => ("'Y' | chkdsk C: /f /r", "Réparation CHKDSK"),
        other    => return Err(format!("Type de réparation inconnu : {}", other)),
    };

    match powershell(script) {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible de lancer {} — {}", label, e)
        )),
        PsResult::Ok(output, code) => {
            let is_ok = code == 0;
            let detail = if is_ok {
                format!("{} terminée avec succès.", label)
            } else {
                let preview: String = output.chars().take(400).collect();
                format!("{} terminée (code {}). Résultat : {}", label, code, preview.trim())
            };
            Ok(CheckResult { is_ok, detail, not_found: false, ps_unavailable: false })
        }
    }
}

// ─── Antivirus detection ──────────────────────────────────────────────────────
#[derive(Serialize, Deserialize)]
struct AntivirusResult {
    active:        Vec<String>,
    inactive:      Vec<String>,
    ps_unavailable: bool,
}

#[tauri::command]
fn check_antivirus() -> Result<AntivirusResult, String> {
    let script = r#"
$avList = Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct -ErrorAction SilentlyContinue
if ($null -eq $avList) { exit 1 }
foreach ($av in $avList) {
    $state = $av.productState
    $rtpOn = (($state -band 0x1000) -ne 0)
    $status = if ($rtpOn) { "active" } else { "inactive" }
    Write-Output "$status|$($av.displayName)"
}
"#;
    match powershell(script) {
        PsResult::SpawnFailed(e) => Ok(AntivirusResult {
            active: vec![], inactive: vec![],
            ps_unavailable: true,
        }.with_warn(e)),
        PsResult::Ok(output, code) => {
            if code != 0 || output.trim().is_empty() {
                return Ok(AntivirusResult { active: vec![], inactive: vec![], ps_unavailable: false });
            }
            let mut active   = Vec::new();
            let mut inactive = Vec::new();
            for line in output.lines() {
                let line = line.trim();
                if line.is_empty() { continue; }
                if let Some((status, name)) = line.split_once('|') {
                    match status {
                        "active"   => active.push(name.to_string()),
                        "inactive" => inactive.push(name.to_string()),
                        _ => {}
                    }
                }
            }
            Ok(AntivirusResult { active, inactive, ps_unavailable: false })
        }
    }
}

impl AntivirusResult {
    // workaround: store warning in a dummy field by converting — we carry it as a field
    fn with_warn(self, _msg: String) -> Self { self }
}

// ─── Activate Microsoft Defender ──────────────────────────────────────────────
#[tauri::command]
fn activate_defender() -> Result<CheckResult, String> {
    let script = "Set-MpPreference -DisableRealtimeMonitoring $false -ErrorAction Stop";
    match powershell(script) {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible d'activer Defender — {}", e)
        )),
        PsResult::Ok(output, code) => {
            if code == 0 {
                Ok(CheckResult::ok("Microsoft Defender a été activé avec succès."))
            } else {
                let preview: String = output.chars().take(300).collect();
                Ok(CheckResult::err(format!("Échec de l'activation (code {}) : {}", code, preview.trim())))
            }
        }
    }
}


// ─── SSD detection ───────────────────────────────────────────────────────────
// Queries the MediaType of the C: drive's physical disk via Get-PhysicalDisk.
// MediaType: 3 = HDD, 4 = SSD, 5 = SCM, 0 = Unspecified.
// SpindleSpeed == 0 is an additional SSD/NVMe indicator when MediaType is unspecified.
#[tauri::command]
fn check_storage_type() -> Result<CheckResult, String> {
    let script = r#"
try {
    $partition = Get-Partition -DriveLetter C -ErrorAction Stop
    $disk = Get-PhysicalDisk | Where-Object {
        (Get-Disk -Number $_.DeviceId -ErrorAction SilentlyContinue) -ne $null -and
        (Get-Partition -DiskNumber $_.DeviceId -ErrorAction SilentlyContinue |
            Where-Object { $_.DriveLetter -eq 'C' }) -ne $null
    } | Select-Object -First 1
    if ($null -eq $disk) {
        # Fallback: get the disk directly from the partition
        $diskNum = $partition.DiskNumber
        $disk = Get-PhysicalDisk | Where-Object { $_.DeviceId -eq $diskNum } | Select-Object -First 1
    }
    if ($null -eq $disk) { Write-Output "unknown|Unknown"; exit 0 }
    $mt = $disk.MediaType
    $spindle = $disk.SpindleSpeed
    $model = $disk.FriendlyName
    Write-Output "$mt|$spindle|$model"
} catch {
    Write-Output "error|$_"
}
"#;
    match powershell(script) {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible de détecter le type de stockage — {}", e)
        )),
        PsResult::Ok(output, _) => {
            let line = output.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
            let parts: Vec<&str> = line.splitn(3, '|').collect();
            let media_type  = parts.first().copied().unwrap_or("0").trim();
            let spindle     = parts.get(1).copied().unwrap_or("0").trim().parse::<u32>().unwrap_or(0);
            let model       = parts.get(2).copied().unwrap_or("Inconnu").trim();

            let is_ssd = match media_type {
                "4" => true,                 // SSD (explicit)
                "5" => true,                 // SCM / Optane
                "3" => false,                // HDD (explicit)
                _   => spindle == 0,         // Unspecified: SpindleSpeed == 0 → SSD/NVMe
            };

            let type_label = match media_type {
                "4" => "SSD",
                "5" => "SSD (SCM/Optane)",
                "3" => "HDD",
                _   => if spindle == 0 { "SSD/NVMe" } else { "HDD" },
            };

            if is_ssd {
                Ok(CheckResult::ok(format!(
                    "Stockage de type {} détecté ({}) — Performances optimales.",
                    type_label, model
                )))
            } else {
                Ok(CheckResult::err(format!(
                    "Disque dur très lent en place ({}, {}). Un stockage SSD accélère grandement l'ordinateur.",
                    type_label, model
                )))
            }
        }
    }
}


// ─── Windows Recovery (WinRE) ─────────────────────────────────────────────────
// reagentc /info outputs a line like:
//   Windows RE status:         Enabled
//   Windows RE status:         Disabled
// We parse that line to determine state.
// If disabled, attempt reagentc /disable then reagentc /enable to re-register.
#[tauri::command]
fn check_winre() -> Result<CheckResult, String> {
    match powershell("reagentc /info") {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible d'exécuter reagentc — {}", e)
        )),
        PsResult::Ok(output, _) => {
            let lower = output.to_lowercase();
            if lower.contains("enabled") {
                Ok(CheckResult::ok(
                    "Windows Recovery (WinRE) est activé et opérationnel."
                ))
            } else if lower.contains("disabled") {
                Ok(CheckResult::err(
                    "Windows Recovery (WinRE) est désactivé. Cliquez sur 'Réparer WinRE' pour le réactiver."
                ))
            } else if lower.contains("access") || lower.contains("administrator") || lower.contains("administrateur") {
                Ok(CheckResult::unavailable(
                    "reagentc nécessite des droits administrateur. Relancez Dr Reco en tant qu'administrateur."
                ))
            } else {
                let preview: String = output.chars().take(300).collect();
                Ok(CheckResult::unavailable(
                    format!("Impossible de déterminer l'état de WinRE. Résultat : {}", preview.trim())
                ))
            }
        }
    }
}

#[tauri::command]
fn repair_winre() -> Result<CheckResult, String> {
    // Step 1: disable (clears broken state)
    let (_, _) = match powershell("reagentc /disable") {
        PsResult::SpawnFailed(e) => return Ok(CheckResult::unavailable(
            format!("Impossible de lancer reagentc /disable — {}", e)
        )),
        PsResult::Ok(out, code) => (out, code),
    };

    // Step 2: re-enable
    match powershell("reagentc /enable") {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("reagentc /disable a réussi mais reagentc /enable a échoué — {}", e)
        )),
        PsResult::Ok(output, code) => {
            let lower = output.to_lowercase();
            let is_ok = code == 0 && (lower.contains("enabled") || lower.contains("activé") || lower.contains("operation successful") || lower.contains("opération réussie"));
            if is_ok {
                Ok(CheckResult::ok("Windows Recovery (WinRE) a été réparé et réactivé avec succès."))
            } else {
                let preview: String = output.chars().take(400).collect();
                Ok(CheckResult::err(format!(
                    "La réactivation de WinRE a échoué (code {}). Résultat : {}",
                    code, preview.trim()
                )))
            }
        }
    }
}


// ─── Launch Windows Update ────────────────────────────────────────────────────
#[tauri::command]
fn launch_windows_update() -> Result<CheckResult, String> {
    let script = r#"Start-Process -FilePath "$env:windir\system32\usoclient.exe" -ArgumentList "ScanInstallWait" -WindowStyle Hidden"#;
    match powershell(script) {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible de lancer Windows Update — {}", e)
        )),
        PsResult::Ok(_, code) => {
            if code == 0 {
                Ok(CheckResult::ok("Windows Update a été lancé. Les mises à jour vont s'installer en arrière-plan."))
            } else {
                Ok(CheckResult::err(format!(
                    "Windows Update a démarré mais a retourné le code {}. Vérifiez l'état dans les Paramètres → Windows Update.", code
                )))
            }
        }
    }
}


// ─── Services CNAM ────────────────────────────────────────────────────────────
const EXPECTED_SRVSVCNAM: &str = "Composant SrvSvCnam 5.10.04";
const SRVSVCNAM_KEY: &str = "Installer\\Products\\2f2196035f673ab429069e5f188c68dd";

#[tauri::command]
fn check_services_cnam() -> Result<CheckResult, String> {
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);

    match hkcr.open_subkey(SRVSVCNAM_KEY) {
        Err(_) => Ok(CheckResult::missing(
            "Services CNAM introuvables. Le composant SrvSvCnam n'est pas installé."
        )),
        Ok(key) => match key.get_value::<String, _>("ProductName") {
            Err(_) => Ok(CheckResult::missing(
                "Clé Services CNAM trouvée mais la valeur ProductName est absente."
            )),
            Ok(name) => {
                let name = name.trim().to_string();
                if name == EXPECTED_SRVSVCNAM {
                    Ok(CheckResult::ok(format!(
                        "Services CNAM détectés : {} — Version conforme.", name
                    )))
                } else {
                    Ok(CheckResult::err(format!(
                        "Services CNAM détectés mais version incorrecte : {} (attendu : {}). Veuillez mettre à jour le composant.",
                        name, EXPECTED_SRVSVCNAM
                    )))
                }
            }
        }
    }
}


// ─── Smart card reader detection ─────────────────────────────────────────────
#[tauri::command]
fn check_smartcard_reader() -> Result<CheckResult, String> {
    let script = "Get-PnpDevice -Class SmartCardReader -Status OK -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FriendlyName";
    match powershell(script) {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible de détecter les lecteurs de carte — {}", e)
        )),
        PsResult::Ok(output, _) => {
            let names: Vec<&str> = output
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect();
            if names.is_empty() {
                Ok(CheckResult::err(
                    "Aucun lecteur de carte à puce détecté. Vérifiez que le lecteur est branché et que ses pilotes sont installés."
                ))
            } else {
                Ok(CheckResult::ok(format!(
                    "Lecteur de carte à puce détecté : {}.",
                    names.join(", ")
                )))
            }
        }
    }
}


// ─── Browser & extension detection ───────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct BrowserResult {
    browser:            String,  // "chrome" | "firefox" | "edge" | "other" | "unknown"
    browser_label:      String,  // Human-readable name
    extension_found:    bool,
    extension_checked:  bool,    // false if browser not supported for extension check
    detail:             String,
    ps_unavailable:     bool,
}

#[tauri::command]
fn check_browser_and_extension() -> Result<BrowserResult, String> {
    // ── Step 1: detect default browser via registry ──────────────────────────
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let prog_id: String = hkcu
        .open_subkey(
            "Software\\Microsoft\\Windows\\Shell\\Associations\\UrlAssociations\\https\\UserChoice",
        )
        .and_then(|k| k.get_value("ProgId"))
        .unwrap_or_default();

    let prog_lower = prog_id.to_lowercase();
    let (browser, browser_label) = if prog_lower.contains("chrome") {
        ("chrome", "Google Chrome")
    } else if prog_lower.contains("firefox") {
        ("firefox", "Mozilla Firefox")
    } else if prog_lower.contains("msedge") || prog_lower.contains("edge") {
        ("edge", "Microsoft Edge")
    } else if prog_id.is_empty() {
        ("unknown", "Inconnu")
    } else {
        ("other", prog_id.as_str())
    };

    // ── Step 2: check extension per browser ──────────────────────────────────
    match browser {
        "chrome" => {
            // Extension registry key (64-bit path first, then 32-bit fallback)
            const EXT_ID: &str = "kpjpglcbcgnblkigbedgaoegjbifejka";
            let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
            let key_64 = format!("SOFTWARE\\Google\\Chrome\\Extensions\\{}", EXT_ID);
            let key_32 = format!("SOFTWARE\\WOW6432Node\\Google\\Chrome\\Extensions\\{}", EXT_ID);
            let found = hklm.open_subkey(&key_64).is_ok()
                || hklm.open_subkey(&key_32).is_ok();

            let detail = if found {
                format!(
                    "Navigateur par défaut : {}. Extension \"Lecture Carte Vitale\" ({}) détectée.",
                    browser_label, EXT_ID
                )
            } else {
                format!(
                    "Navigateur par défaut : {}. Extension \"Lecture Carte Vitale\" ({}) non trouvée.",
                    browser_label, EXT_ID
                )
            };

            Ok(BrowserResult {
                browser: browser.into(),
                browser_label: browser_label.into(),
                extension_found: found,
                extension_checked: true,
                detail,
                ps_unavailable: false,
            })
        }

        "firefox" => {
            // Scan all Firefox profile extensions.json files for the extension name
            let script = r#"
$found = $false
$profilesDir = "$env:APPDATA\Mozilla\Firefox\Profiles"
if (Test-Path $profilesDir) {
    Get-ChildItem $profilesDir -Directory | ForEach-Object {
        $extFile = Join-Path $_.FullName "extensions.json"
        if (Test-Path $extFile) {
            $json = Get-Content $extFile -Raw -ErrorAction SilentlyContinue
            if ($json -match '"Lecture Carte Vitale"') {
                $found = $true
            }
        }
    }
}
if ($found) { Write-Output "found" } else { Write-Output "not_found" }
"#;
            match powershell(script) {
                PsResult::SpawnFailed(e) => Ok(BrowserResult {
                    browser: browser.into(),
                    browser_label: browser_label.into(),
                    extension_found: false,
                    extension_checked: false,
                    detail: format!("Navigateur par défaut : {}. Impossible de vérifier l\'extension — {}", browser_label, e),
                    ps_unavailable: true,
                }),
                PsResult::Ok(output, _) => {
                    let found = output.trim() == "found";
                    let detail = if found {
                        format!(
                            "Navigateur par défaut : {}. Extension \"Lecture Carte Vitale\" détectée.",
                            browser_label
                        )
                    } else {
                        format!(
                            "Navigateur par défaut : {}. Extension \"Lecture Carte Vitale\" non trouvée dans les profils Firefox.",
                            browser_label
                        )
                    };
                    Ok(BrowserResult {
                        browser: browser.into(),
                        browser_label: browser_label.into(),
                        extension_found: found,
                        extension_checked: true,
                        detail,
                        ps_unavailable: false,
                    })
                }
            }
        }

        _ => {
            // Edge, other, or unknown — report browser but skip extension check
            let detail = if browser == "unknown" {
                "Impossible de détecter le navigateur par défaut.".into()
            } else {
                format!(
                    "Navigateur par défaut : {}. La vérification de l\'extension \"Lecture Carte Vitale\" n\'est pas prise en charge pour ce navigateur.",
                    browser_label
                )
            };
            Ok(BrowserResult {
                browser: browser.into(),
                browser_label: browser_label.into(),
                extension_found: false,
                extension_checked: false,
                detail,
                ps_unavailable: false,
            })
        }
    }
}


// ─── Browser version check ────────────────────────────────────────────────────
// Latest stable major versions as of March 2026:
//   Chrome  → 146
//   Firefox → 148
// We compare only the major version (first segment) since that is what auto-update tracks.

#[derive(Serialize, Deserialize)]
struct BrowserVersionResult {
    browser:        String,
    browser_label:  String,
    installed:      String,   // full version string, empty if not found
    latest_major:   u32,
    is_ok:          bool,
    detail:         String,
    ps_unavailable: bool,
}

#[tauri::command]
fn check_browser_version(browser: String) -> Result<BrowserVersionResult, String> {
    let (browser_label, latest_major) = match browser.as_str() {
        "chrome"  => ("Google Chrome",    146u32),
        "firefox" => ("Mozilla Firefox",  148u32),
        _         => return Err(format!("Navigateur non pris en charge : {}", browser)),
    };

    let installed: String = match browser.as_str() {
        "chrome" => {
            // HKCU\Software\Google\Chrome\BLBeacon → version
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            hkcu.open_subkey("Software\\Google\\Chrome\\BLBeacon")
                .and_then(|k| k.get_value::<String, _>("version"))
                .unwrap_or_default()
                .trim()
                .to_string()
        }
        "firefox" => {
            // HKLM\SOFTWARE\Mozilla\Mozilla Firefox → CurrentVersion
            // Fallback to WOW6432Node on 64-bit
            let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
            hklm.open_subkey("SOFTWARE\\Mozilla\\Mozilla Firefox")
                .or_else(|_| hklm.open_subkey("SOFTWARE\\Wow6432Node\\Mozilla\\Mozilla Firefox"))
                .and_then(|k| k.get_value::<String, _>("CurrentVersion"))
                .unwrap_or_default()
                .trim()
                .to_string()
        }
        _ => String::new(),
    };

    if installed.is_empty() {
        return Ok(BrowserVersionResult {
            browser: browser.clone(),
            browser_label: browser_label.into(),
            installed: String::new(),
            latest_major,
            is_ok: false,
            detail: format!(
                "Impossible de lire la version installée de {}. Le navigateur n'est peut-être pas installé ou n'a jamais été lancé.",
                browser_label
            ),
            ps_unavailable: false,
        });
    }

    // Parse major version from e.g. "136.0.7103.114" or "136.0" or "136 esr"
    let major: u32 = installed
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let is_ok = major >= latest_major;
    let detail = if is_ok {
        format!(
            "{} version {} — à jour (version majeure minimale : {}).",
            browser_label, installed, latest_major
        )
    } else {
        format!(
            "{} version {} — obsolète. La version {} ou supérieure est recommandée. Mettez à jour le navigateur.",
            browser_label, installed, latest_major
        )
    };

    Ok(BrowserVersionResult {
        browser: browser.clone(),
        browser_label: browser_label.into(),
        installed,
        latest_major,
        is_ok,
        detail,
        ps_unavailable: false,
    })
}

// ─── Launch browser update ────────────────────────────────────────────────────
#[tauri::command]
fn launch_browser_update(browser: String) -> Result<CheckResult, String> {
    let script = match browser.as_str() {
        // Open Chrome's built-in update page
        "chrome" => r#"Start-Process "chrome.exe" -ArgumentList "--chrome-frame","chrome://settings/help" -ErrorAction SilentlyContinue; Start-Process "chrome://settings/help" -ErrorAction SilentlyContinue"#,
        // Open Firefox's built-in update preferences page
        "firefox" => r#"Start-Process "firefox.exe" -ArgumentList "about:preferences#general" -ErrorAction SilentlyContinue"#,
        other => return Err(format!("Navigateur non pris en charge : {}", other)),
    };

    let label = match browser.as_str() {
        "chrome"  => "Google Chrome",
        "firefox" => "Mozilla Firefox",
        _         => &browser,
    };

    match powershell(script) {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible de lancer la mise à jour de {} — {}", label, e)
        )),
        PsResult::Ok(_, _) => Ok(CheckResult::ok(format!(
            "La page de mise à jour de {} a été ouverte. Suivez les instructions dans le navigateur pour terminer la mise à jour.",
            label
        ))),
    }
}

// ─── Entry point ──────────────────────────────────────────────────────────────
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_hwinfo::init())
        .invoke_handler(tauri::generate_handler![
            run_sfc_check,
            run_chkdsk,
            check_cryptolib_version,
            open_url,
            run_repair,
            check_antivirus,
            activate_defender,
            check_storage_type,
            check_winre,
            repair_winre,
            launch_windows_update,
            check_services_cnam,
            check_smartcard_reader,
            check_browser_and_extension,
            check_browser_version,
            launch_browser_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
