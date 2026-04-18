#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use tauri_plugin_shell::ShellExt;
use winreg::enums::*;
use winreg::RegKey;

// ─── Shared types ─────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct CheckResult {
    is_ok: bool,
    detail: String,
    #[serde(default)]
    not_found: bool,
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

// ─── PowerShell helpers ───────────────────────────────────────────────────────

enum PsResult {
    Ok(String, i32),
    SpawnFailed(String),
}

/// Run an arbitrary PowerShell script.
/// Forces UTF-8 output encoding so accented characters are not mangled.
async fn powershell(app: &tauri::AppHandle, script: &str) -> PsResult {
    let utf8_prefix = "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
                       $OutputEncoding = [System.Text.Encoding]::UTF8; ";
    let full_script = format!("{}{}", utf8_prefix, script);

    let result = app.shell()
        .command("powershell.exe")
        .args([
            "-NonInteractive",
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-Command", &full_script,
        ])
        .output()
        .await;

    match result {
        Err(e) => PsResult::SpawnFailed(format!(
            "PowerShell est introuvable ou inaccessible : {}. Vérifiez que PowerShell est installé sur ce système.",
            e
        )),
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            PsResult::Ok(
                format!("{}{}", stdout, stderr),
                output.status.code().unwrap_or(-1),
            )
        }
    }
}

/// Wrap a native Windows binary (sfc, chkdsk, reagentc…) inside a PowerShell
/// script that re-encodes its OEM-codepage output as UTF-8.
async fn powershell_native(app: &tauri::AppHandle, command: &str) -> PsResult {
    let script = format!(r#"
$tmp = "$env:TEMP\dr_reco_native_out.txt"
$oem = [System.Text.Encoding]::GetEncoding([System.Globalization.CultureInfo]::CurrentCulture.TextInfo.OEMCodePage)
$proc = Start-Process -FilePath "cmd.exe" `
    -ArgumentList "/c {command} 2>&1" `
    -RedirectStandardOutput $tmp `
    -NoNewWindow -Wait -PassThru
$exitCode = $proc.ExitCode
if (Test-Path $tmp) {{
    $raw = [System.IO.File]::ReadAllBytes($tmp)
    $text = $oem.GetString($raw)
    Remove-Item $tmp -Force -ErrorAction SilentlyContinue
    Write-Output $text
}}
exit $exitCode
"#, command = command);

    powershell(app, &script).await
}

// ─── SFC /scannow ─────────────────────────────────────────────────────────────
#[tauri::command]
async fn run_sfc_check(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let (combined, _) = match powershell_native(&app, "sfc /scannow").await {
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
        let _ = powershell(&app, "Start-Process -FilePath 'dism.exe' -ArgumentList '/Online','/Cleanup-Image','/RestoreHealth' -WindowStyle Hidden").await;
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
async fn run_chkdsk(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let (combined, exit_code) = match powershell_native(&app, "chkdsk C:").await {
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
        let _ = powershell_native(&app, "echo Y | chkdsk C: /f /r /x").await;
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
async fn open_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    match powershell(&app, &format!("Start-Process '{}'", url)).await {
        PsResult::SpawnFailed(e) => Err(e),
        PsResult::Ok(_, _) => Ok(()),
    }
}

// ─── Repair commands ──────────────────────────────────────────────────────────
#[tauri::command]
async fn run_repair(app: tauri::AppHandle, kind: String) -> Result<CheckResult, String> {
    let (script, label) = match kind.as_str() {
        "sfc"    => ("dism /online /cleanup-image /restorehealth", "Réparation DISM"),
        "chkdsk" => ("'Y' | chkdsk C: /f /r", "Réparation CHKDSK"),
        other    => return Err(format!("Type de réparation inconnu : {}", other)),
    };

    match powershell(&app, script).await {
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
    active:         Vec<String>,
    inactive:       Vec<String>,
    ps_unavailable: bool,
}

#[tauri::command]
async fn check_antivirus(app: tauri::AppHandle) -> Result<AntivirusResult, String> {
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
    match powershell(&app, script).await {
        PsResult::SpawnFailed(_) => Ok(AntivirusResult { active: vec![], inactive: vec![], ps_unavailable: true }),
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

// ─── Activate Microsoft Defender ──────────────────────────────────────────────
#[tauri::command]
async fn activate_defender(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let script = "Set-MpPreference -DisableRealtimeMonitoring $false -ErrorAction Stop";
    match powershell(&app, script).await {
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
#[tauri::command]
async fn check_storage_type(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let script = r#"
try {
    $partition = Get-Partition -DriveLetter C -ErrorAction Stop
    $disk = Get-PhysicalDisk | Where-Object {
        (Get-Disk -Number $_.DeviceId -ErrorAction SilentlyContinue) -ne $null -and
        (Get-Partition -DiskNumber $_.DeviceId -ErrorAction SilentlyContinue |
            Where-Object { $_.DriveLetter -eq 'C' }) -ne $null
    } | Select-Object -First 1
    if ($null -eq $disk) {
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
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible de détecter le type de stockage — {}", e)
        )),
        PsResult::Ok(output, _) => {
            let line = output.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
            let parts: Vec<&str> = line.splitn(3, '|').collect();
            let media_type = parts.first().copied().unwrap_or("0").trim();
            let spindle    = parts.get(1).copied().unwrap_or("0").trim().parse::<u32>().unwrap_or(0);
            let model      = parts.get(2).copied().unwrap_or("Inconnu").trim();

            let is_ssd = match media_type {
                "4" | "5" => true,
                "3"       => false,
                _         => spindle == 0,
            };
            let type_label = match media_type {
                "4" => "SSD",
                "5" => "SSD (SCM/Optane)",
                "3" => "HDD",
                _   => if spindle == 0 { "SSD/NVMe" } else { "HDD" },
            };

            if is_ssd {
                Ok(CheckResult::ok(format!("Stockage de type {} détecté ({}) — Performances optimales.", type_label, model)))
            } else {
                Ok(CheckResult::err(format!("Disque dur très lent en place ({}, {}). Un stockage SSD accélère grandement l'ordinateur.", type_label, model)))
            }
        }
    }
}

// ─── Windows Recovery (WinRE) ─────────────────────────────────────────────────
#[tauri::command]
async fn check_winre(app: tauri::AppHandle) -> Result<CheckResult, String> {
    match powershell_native(&app, "reagentc /info").await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible d'exécuter reagentc — {}", e)
        )),
        PsResult::Ok(output, _) => {
            let lower = output.to_lowercase();
            if lower.contains("enabled") {
                Ok(CheckResult::ok("Windows Recovery (WinRE) est activé et opérationnel."))
            } else if lower.contains("disabled") {
                Ok(CheckResult::err("Windows Recovery (WinRE) est désactivé. Cliquez sur 'Réparer WinRE' pour le réactiver."))
            } else if lower.contains("access") || lower.contains("administrator") || lower.contains("administrateur") {
                Ok(CheckResult::unavailable("reagentc nécessite des droits administrateur. Relancez Dr Reco en tant qu'administrateur."))
            } else {
                let preview: String = output.chars().take(300).collect();
                Ok(CheckResult::unavailable(format!("Impossible de déterminer l'état de WinRE. Résultat : {}", preview.trim())))
            }
        }
    }
}

#[tauri::command]
async fn repair_winre(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let (_, _) = match powershell_native(&app, "reagentc /disable").await {
        PsResult::SpawnFailed(e) => return Ok(CheckResult::unavailable(
            format!("Impossible de lancer reagentc /disable — {}", e)
        )),
        PsResult::Ok(out, code) => (out, code),
    };

    match powershell_native(&app, "reagentc /enable").await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("reagentc /disable a réussi mais reagentc /enable a échoué — {}", e)
        )),
        PsResult::Ok(output, code) => {
            let lower = output.to_lowercase();
            let is_ok = code == 0 && (lower.contains("enabled") || lower.contains("activé")
                || lower.contains("operation successful") || lower.contains("opération réussie"));
            if is_ok {
                Ok(CheckResult::ok("Windows Recovery (WinRE) a été réparé et réactivé avec succès."))
            } else {
                let preview: String = output.chars().take(400).collect();
                Ok(CheckResult::err(format!("La réactivation de WinRE a échoué (code {}). Résultat : {}", code, preview.trim())))
            }
        }
    }
}

// ─── Launch Windows Update ────────────────────────────────────────────────────
#[tauri::command]
async fn launch_windows_update(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let script = r#"Start-Process -FilePath "$env:windir\system32\usoclient.exe" -ArgumentList "ScanInstallWait" -WindowStyle Hidden"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible de lancer Windows Update — {}", e)
        )),
        PsResult::Ok(_, code) => {
            if code == 0 {
                Ok(CheckResult::ok("Windows Update a été lancé. Les mises à jour vont s'installer en arrière-plan."))
            } else {
                Ok(CheckResult::err(format!("Windows Update a démarré mais a retourné le code {}. Vérifiez l'état dans les Paramètres → Windows Update.", code)))
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
        Err(_) => Ok(CheckResult::missing("Services CNAM introuvables. Le composant SrvSvCnam n'est pas installé.")),
        Ok(key) => match key.get_value::<String, _>("ProductName") {
            Err(_) => Ok(CheckResult::missing("Clé Services CNAM trouvée mais la valeur ProductName est absente.")),
            Ok(name) => {
                let name = name.trim().to_string();
                if name == EXPECTED_SRVSVCNAM {
                    Ok(CheckResult::ok(format!("Services CNAM détectés : {} — Version conforme.", name)))
                } else {
                    Ok(CheckResult::err(format!(
                        "Services CNAM détectés mais version incorrecte : \"{}\" (attendu : \"{}\"). Veuillez mettre à jour le composant.",
                        name, EXPECTED_SRVSVCNAM
                    )))
                }
            }
        }
    }
}

// ─── Smart card reader detection ─────────────────────────────────────────────
#[tauri::command]
async fn check_smartcard_reader(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let script = "Get-PnpDevice -Class SmartCardReader -Status OK -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FriendlyName";
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible de détecter les lecteurs de carte — {}", e)
        )),
        PsResult::Ok(output, _) => {
            let names: Vec<&str> = output.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
            if names.is_empty() {
                Ok(CheckResult::err("Aucun lecteur de carte à puce détecté. Vérifiez que le lecteur est branché et que ses pilotes sont installés."))
            } else {
                Ok(CheckResult::ok(format!("Lecteur de carte à puce détecté : {}.", names.join(", "))))
            }
        }
    }
}

// ─── Browser & extension detection ───────────────────────────────────────────
#[derive(Serialize, Deserialize)]
struct BrowserResult {
    browser:           String,
    browser_label:     String,
    extension_found:   bool,
    extension_checked: bool,
    detail:            String,
    ps_unavailable:    bool,
}

#[tauri::command]
async fn check_browser_and_extension(app: tauri::AppHandle) -> Result<BrowserResult, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let prog_id: String = hkcu
        .open_subkey("Software\\Microsoft\\Windows\\Shell\\Associations\\UrlAssociations\\https\\UserChoice")
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

    match browser {
        "chrome" => {
            const EXT_ID: &str = "kpjpglcbcgnblkigbedgaoegjbifejka";
            let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
            let found = hklm.open_subkey(format!("SOFTWARE\\Google\\Chrome\\Extensions\\{}", EXT_ID)).is_ok()
                || hklm.open_subkey(format!("SOFTWARE\\WOW6432Node\\Google\\Chrome\\Extensions\\{}", EXT_ID)).is_ok();
            let detail = if found {
                format!("Navigateur par défaut : {}. Extension \"Lecture Carte Vitale\" ({}) détectée.", browser_label, EXT_ID)
            } else {
                format!("Navigateur par défaut : {}. Extension \"Lecture Carte Vitale\" ({}) non trouvée.", browser_label, EXT_ID)
            };
            Ok(BrowserResult { browser: browser.into(), browser_label: browser_label.into(), extension_found: found, extension_checked: true, detail, ps_unavailable: false })
        }

        "firefox" => {
            let script = r#"
$found = $false
$profilesDir = "$env:APPDATA\Mozilla\Firefox\Profiles"
if (Test-Path $profilesDir) {
    Get-ChildItem $profilesDir -Directory | ForEach-Object {
        $extFile = Join-Path $_.FullName "extensions.json"
        if (Test-Path $extFile) {
            $json = Get-Content $extFile -Raw -ErrorAction SilentlyContinue
            if ($json -match '"Lecture Carte Vitale"') { $found = $true }
        }
    }
}
if ($found) { Write-Output "found" } else { Write-Output "not_found" }
"#;
            match powershell(&app, script).await {
                PsResult::SpawnFailed(e) => Ok(BrowserResult {
                    browser: browser.into(), browser_label: browser_label.into(),
                    extension_found: false, extension_checked: false,
                    detail: format!("Navigateur par défaut : {}. Impossible de vérifier l'extension — {}", browser_label, e),
                    ps_unavailable: true,
                }),
                PsResult::Ok(output, _) => {
                    let found = output.trim() == "found";
                    let detail = if found {
                        format!("Navigateur par défaut : {}. Extension \"Lecture Carte Vitale\" détectée.", browser_label)
                    } else {
                        format!("Navigateur par défaut : {}. Extension \"Lecture Carte Vitale\" non trouvée dans les profils Firefox.", browser_label)
                    };
                    Ok(BrowserResult { browser: browser.into(), browser_label: browser_label.into(), extension_found: found, extension_checked: true, detail, ps_unavailable: false })
                }
            }
        }

        "edge" => {
            const EXT_ID: &str = "kpjpglcbcgnblkigbedgaoegjbifejka";
            let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
            let found = hklm.open_subkey(format!("SOFTWARE\\Microsoft\\Edge\\Extensions\\{}", EXT_ID)).is_ok()
                || hklm.open_subkey(format!("SOFTWARE\\WOW6432Node\\Microsoft\\Edge\\Extensions\\{}", EXT_ID)).is_ok();
            let detail = if found {
                format!("Navigateur par défaut : {}. Extension \"Lecture Carte Vitale\" ({}) détectée.", browser_label, EXT_ID)
            } else {
                format!("Navigateur par défaut : {}. Extension \"Lecture Carte Vitale\" ({}) non trouvée.", browser_label, EXT_ID)
            };
            Ok(BrowserResult { browser: browser.into(), browser_label: browser_label.into(), extension_found: found, extension_checked: true, detail, ps_unavailable: false })
        }

        _ => {
            let detail = if browser == "unknown" {
                "Impossible de détecter le navigateur par défaut.".into()
            } else {
                format!("Navigateur par défaut : {}. La vérification de l'extension \"Lecture Carte Vitale\" n'est pas prise en charge pour ce navigateur.", browser_label)
            };
            Ok(BrowserResult { browser: browser.into(), browser_label: browser_label.into(), extension_found: false, extension_checked: false, detail, ps_unavailable: false })
        }
    }
}

// ─── Browser version check ────────────────────────────────────────────────────
#[derive(Serialize, Deserialize)]
struct BrowserVersionResult {
    browser:       String,
    browser_label: String,
    installed:     String,
    latest_major:  u32,
    is_ok:         bool,
    detail:        String,
    ps_unavailable: bool,
}

#[tauri::command]
fn check_browser_version(browser: String) -> Result<BrowserVersionResult, String> {
    let (browser_label, latest_major) = match browser.as_str() {
        "chrome"  => ("Google Chrome",   146u32),
        "firefox" => ("Mozilla Firefox", 148u32),
        "edge"    => ("Microsoft Edge",  134u32),
        _         => return Err(format!("Navigateur non pris en charge : {}", browser)),
    };

    let installed: String = match browser.as_str() {
        "chrome" => {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            hkcu.open_subkey("Software\\Google\\Chrome\\BLBeacon")
                .and_then(|k| k.get_value::<String, _>("version"))
                .unwrap_or_default().trim().to_string()
        }
        "firefox" => {
            let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
            hklm.open_subkey("SOFTWARE\\Mozilla\\Mozilla Firefox")
                .or_else(|_| hklm.open_subkey("SOFTWARE\\Wow6432Node\\Mozilla\\Mozilla Firefox"))
                .and_then(|k| k.get_value::<String, _>("CurrentVersion"))
                .unwrap_or_default().trim().to_string()
        }
        "edge" => {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            hkcu.open_subkey("Software\\Microsoft\\Edge\\BLBeacon")
                .and_then(|k| k.get_value::<String, _>("version"))
                .unwrap_or_default().trim().to_string()
        }
        _ => String::new(),
    };

    if installed.is_empty() {
        return Ok(BrowserVersionResult {
            browser: browser.clone(), browser_label: browser_label.into(),
            installed: String::new(), latest_major, is_ok: false,
            detail: format!("Impossible de lire la version installée de {}. Le navigateur n'est peut-être pas installé ou n'a jamais été lancé.", browser_label),
            ps_unavailable: false,
        });
    }

    let major: u32 = installed.split(|c: char| !c.is_ascii_digit()).next()
        .and_then(|s| s.parse().ok()).unwrap_or(0);
    let is_ok = major >= latest_major;
    let detail = if is_ok {
        format!("{} version {} — à jour (version majeure minimale : {}).", browser_label, installed, latest_major)
    } else {
        format!("{} version {} — obsolète. La version {} ou supérieure est recommandée. Mettez à jour le navigateur.", browser_label, installed, latest_major)
    };

    Ok(BrowserVersionResult { browser: browser.clone(), browser_label: browser_label.into(), installed, latest_major, is_ok, detail, ps_unavailable: false })
}

// ─── Launch browser update ────────────────────────────────────────────────────
#[tauri::command]
async fn launch_browser_update(app: tauri::AppHandle, browser: String) -> Result<CheckResult, String> {
    let (script, label) = match browser.as_str() {
        "chrome"  => (r#"Start-Process "chrome.exe" -ArgumentList "--chrome-frame","chrome://settings/help" -ErrorAction SilentlyContinue; Start-Process "chrome://settings/help" -ErrorAction SilentlyContinue"#, "Google Chrome"),
        "firefox" => (r#"Start-Process "firefox.exe" -ArgumentList "about:preferences#general" -ErrorAction SilentlyContinue"#, "Mozilla Firefox"),
        "edge"    => (r#"Start-Process "msedge.exe" -ArgumentList "edge://settings/help" -ErrorAction SilentlyContinue"#, "Microsoft Edge"),
        other     => return Err(format!("Navigateur non pris en charge : {}", other)),
    };

    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(format!("Impossible de lancer la mise à jour de {} — {}", label, e))),
        PsResult::Ok(_, _) => Ok(CheckResult::ok(format!("La page de mise à jour de {} a été ouverte. Suivez les instructions dans le navigateur pour terminer la mise à jour.", label))),
    }
}

// ─── Windows Fast Startup ─────────────────────────────────────────────────────
#[tauri::command]
fn check_fast_startup() -> Result<CheckResult, String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    match hklm.open_subkey("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Power") {
        Err(_) => Ok(CheckResult::err("Démarrage rapide désactivé (clé de registre introuvable).")),
        Ok(key) => {
            let value: u32 = key.get_value("HiberbootEnabled").unwrap_or(0);
            if value == 1 {
                Ok(CheckResult::ok("Démarrage rapide Windows activé — Le système démarre plus rapidement."))
            } else {
                Ok(CheckResult::err("Démarrage rapide désactivé. L'activer accélère le démarrage de l'ordinateur."))
            }
        }
    }
}

#[tauri::command]
async fn enable_fast_startup(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let script = r#"
powercfg /hibernate on
Set-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power" -Name "HiberbootEnabled" -Value 1 -Type DWord -Force
"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(format!("Impossible d'activer le démarrage rapide — {}", e))),
        PsResult::Ok(_, code) => {
            if code == 0 {
                Ok(CheckResult::ok("Démarrage rapide activé avec succès. Le changement sera effectif au prochain redémarrage."))
            } else {
                Ok(CheckResult::err(format!("L'activation du démarrage rapide a échoué (code {}). Des droits administrateur sont peut-être nécessaires.", code)))
            }
        }
    }
}

// ─── Battery health ───────────────────────────────────────────────────────────
#[derive(Serialize, Deserialize)]
struct BatteryResult {
    is_laptop:      bool,
    has_battery:    bool,
    design_mwh:     u64,
    full_mwh:       u64,
    health_pct:     u32,
    is_ok:          bool,
    detail:         String,
    ps_unavailable: bool,
}

#[tauri::command]
async fn check_battery_health(app: tauri::AppHandle) -> Result<BatteryResult, String> {
    let laptop_script = r#"
$chassis = (Get-CimInstance -ClassName Win32_SystemEnclosure -ErrorAction SilentlyContinue).ChassisTypes
$portable = @(8,9,10,11,14,30,31,32)
$isLaptop = $false
foreach ($c in $chassis) { if ($portable -contains $c) { $isLaptop = $true } }
if ($isLaptop) { Write-Output "laptop" } else { Write-Output "desktop" }
"#;
    let is_laptop = match powershell(&app, laptop_script).await {
        PsResult::SpawnFailed(e) => return Ok(BatteryResult {
            is_laptop: false, has_battery: false, design_mwh: 0, full_mwh: 0,
            health_pct: 0, is_ok: true,
            detail: format!("Impossible de détecter le type de machine — {}", e),
            ps_unavailable: true,
        }),
        PsResult::Ok(output, _) => output.trim() == "laptop",
    };

    if !is_laptop {
        return Ok(BatteryResult {
            is_laptop: false, has_battery: false, design_mwh: 0, full_mwh: 0,
            health_pct: 0, is_ok: true, detail: String::new(), ps_unavailable: false,
        });
    }

    let report_script = r#"
$tmp = "$env:TEMP\dr_reco_battery.xml"
powercfg /batteryreport /xml /output $tmp 2>$null | Out-Null
if (-not (Test-Path $tmp)) { Write-Output "no_file"; exit 0 }
try {
    [xml]$xml = Get-Content $tmp -ErrorAction Stop
    $batteries = $xml.BatteryReport.Batteries.Battery
    if ($null -eq $batteries) { Write-Output "no_battery"; exit 0 }
    $design = 0; $full = 0
    foreach ($b in @($batteries)) {
        $design += [int64]$b.DesignCapacity
        $full   += [int64]$b.FullChargeCapacity
    }
    Write-Output "$design|$full"
} catch {
    Write-Output "error|$_"
} finally {
    Remove-Item $tmp -Force -ErrorAction SilentlyContinue
}
"#;

    match powershell(&app, report_script).await {
        PsResult::SpawnFailed(e) => Ok(BatteryResult {
            is_laptop: true, has_battery: false, design_mwh: 0, full_mwh: 0,
            health_pct: 0, is_ok: false,
            detail: format!("Impossible de générer le rapport batterie — {}", e),
            ps_unavailable: true,
        }),
        PsResult::Ok(output, _) => {
            let line = output.trim();
            match line {
                "no_file" => Ok(BatteryResult { is_laptop: true, has_battery: false, design_mwh: 0, full_mwh: 0, health_pct: 0, is_ok: false, detail: "Le rapport batterie n'a pas pu être généré. Des droits administrateur sont peut-être nécessaires.".into(), ps_unavailable: false }),
                "no_battery" => Ok(BatteryResult { is_laptop: true, has_battery: false, design_mwh: 0, full_mwh: 0, health_pct: 0, is_ok: true, detail: "Aucune batterie détectée dans le rapport (ordinateur portable sur secteur uniquement ?).".into(), ps_unavailable: false }),
                _ if line.starts_with("error|") => Ok(BatteryResult { is_laptop: true, has_battery: false, design_mwh: 0, full_mwh: 0, health_pct: 0, is_ok: false, detail: format!("Erreur lors de l'analyse du rapport : {}", &line[6..]), ps_unavailable: false }),
                _ => {
                    let parts: Vec<&str> = line.splitn(2, '|').collect();
                    let design: u64 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let full:   u64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                    if design == 0 {
                        return Ok(BatteryResult { is_laptop: true, has_battery: false, design_mwh: 0, full_mwh: 0, health_pct: 0, is_ok: false, detail: "Capacité nominale nulle — données batterie invalides.".into(), ps_unavailable: false });
                    }
                    let health_pct = ((full as f64 / design as f64) * 100.0).round() as u32;
                    let condition = if health_pct >= 80 { "Bonne santé" } else if health_pct >= 50 { "Usure normale" } else { "Batterie fortement dégradée" };
                    Ok(BatteryResult {
                        is_laptop: true, has_battery: true, design_mwh: design, full_mwh: full,
                        health_pct, is_ok: health_pct >= 50,
                        detail: format!("Santé batterie : {}% ({} mWh / {} mWh nominaux) — {}.", health_pct, full, design, condition),
                        ps_unavailable: false,
                    })
                }
            }
        }
    }
}

// ─── Quick Machine Recovery (QMR) ────────────────────────────────────────────
#[tauri::command]
async fn check_qmr(app: tauri::AppHandle) -> Result<CheckResult, String> {
    match powershell_native(&app, "reagentc /getrecoverysettings").await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(format!("Impossible d'exécuter reagentc — {}", e))),
        PsResult::Ok(output, code) => {
            let lower = output.to_lowercase();
            if code != 0 && (lower.contains("not recognized") || lower.contains("unknown")
                || lower.contains("n'est pas reconnu") || lower.contains("getrecoverysettings")) {
                return Ok(CheckResult { is_ok: true,
                    detail: "Récupération machine rapide non disponible sur cette version de Windows (requiert Windows 11 24H2 build 26100.4700 ou supérieur).".into(),
                    not_found: true, ps_unavailable: false });
            }
            let cloud_on = lower.contains(r#"cloudremediation state="1""#);
            let auto_on  = lower.contains(r#"autoremediation state="1""#);
            if cloud_on {
                let detail = if auto_on {
                    "Récupération machine rapide activée — Correction cloud et correction automatique activées."
                } else {
                    "Récupération machine rapide activée — Correction cloud activée, correction automatique désactivée."
                };
                Ok(CheckResult::ok(detail))
            } else {
                Ok(CheckResult::err("Récupération machine rapide désactivée. La correction cloud n'est pas configurée."))
            }
        }
    }
}

#[tauri::command]
async fn enable_qmr(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let script = r#"
$xml = "<?xml version='1.0' encoding='utf-8'?><WindowsRE><CloudRemediation state=""1"" /><AutoRemediation state=""1"" totalwaittime=""2400"" waitinterval=""120""/></WindowsRE>"
$tmp = "$env:TEMP\dr_reco_qmr.xml"
$xml | Out-File -FilePath $tmp -Encoding utf8 -Force
$result = & reagentc /setrecoverysettings $tmp 2>&1
Remove-Item $tmp -Force -ErrorAction SilentlyContinue
Write-Output $result
"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(format!("Impossible d'activer la récupération machine rapide — {}", e))),
        PsResult::Ok(output, code) => {
            let lower = output.to_lowercase();
            let success = code == 0 || lower.contains("operation successful") || lower.contains("opération réussie");
            if success {
                Ok(CheckResult::ok("Récupération machine rapide activée avec succès (correction cloud + automatique)."))
            } else {
                let preview: String = output.chars().take(400).collect();
                Ok(CheckResult::err(format!("L'activation a échoué (code {}). Résultat : {}", code, preview.trim())))
            }
        }
    }
}

// ─── USB Selective Suspend ────────────────────────────────────────────────────
#[tauri::command]
async fn check_usb_suspend(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let script = r#"
$sub  = "2a737441-1930-4402-8d77-b2bea1222653"
$set  = "48e6b7a6-50f5-4782-a5d4-53bb8f07e226"
$q    = powercfg /QUERY SCHEME_CURRENT $sub $set 2>&1
$ac   = ($q | Select-String "Current AC Power Setting Index").ToString() -replace '.*:\s*0x0*',''
$dc   = ($q | Select-String "Current DC Power Setting Index").ToString() -replace '.*:\s*0x0*',''
Write-Output "ac=$ac dc=$dc"
"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(format!("Impossible de lire la configuration USB — {}", e))),
        PsResult::Ok(output, _) => {
            let line = output.trim();
            let ac_on = line.contains("ac=1");
            let dc_on = line.contains("dc=1");
            if !line.contains("ac=") {
                return Ok(CheckResult { is_ok: true,
                    detail: "La mise en veille sélective USB n'est pas disponible sur ce plan d'alimentation.".into(),
                    not_found: true, ps_unavailable: false });
            }
            if ac_on || dc_on {
                let which = match (ac_on, dc_on) {
                    (true,  true)  => "secteur et batterie",
                    (true,  false) => "secteur uniquement",
                    (false, true)  => "batterie uniquement",
                    _              => "",
                };
                Ok(CheckResult::err(format!(
                    "Mise en veille sélective USB activée ({}) — Windows peut couper l'alimentation des périphériques USB, ce qui peut provoquer des déconnexions de lecteurs de carte.",
                    which
                )))
            } else {
                Ok(CheckResult::ok("Mise en veille sélective USB désactivée — Les périphériques USB restent alimentés en permanence."))
            }
        }
    }
}

#[tauri::command]
async fn disable_usb_suspend(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let script = r#"
powercfg /SETACVALUEINDEX SCHEME_CURRENT 2a737441-1930-4402-8d77-b2bea1222653 48e6b7a6-50f5-4782-a5d4-53bb8f07e226 0
powercfg /SETDCVALUEINDEX SCHEME_CURRENT 2a737441-1930-4402-8d77-b2bea1222653 48e6b7a6-50f5-4782-a5d4-53bb8f07e226 0
powercfg /SETACTIVE SCHEME_CURRENT
"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(format!("Impossible de modifier le plan d'alimentation — {}", e))),
        PsResult::Ok(_, code) => {
            if code == 0 {
                Ok(CheckResult::ok("Mise en veille sélective USB désactivée avec succès. Les périphériques USB resteront alimentés en permanence."))
            } else {
                Ok(CheckResult::err(format!("La désactivation a échoué (code {}). Des droits administrateur sont peut-être nécessaires.", code)))
            }
        }
    }
}

// ─── Entry point ──────────────────────────────────────────────────────────────
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
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
            check_fast_startup,
            enable_fast_startup,
            check_battery_health,
            check_qmr,
            enable_qmr,
            check_usb_suspend,
            disable_usb_suspend,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
