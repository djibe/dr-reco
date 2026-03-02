#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::process::Command;
use std::os::windows::process::CommandExt;
use winreg::enums::*;
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Serialize, Deserialize)]
struct CheckResult {
    is_ok: bool,
    detail: String,
    #[serde(default)]
    not_found: bool,
}

// Run a PowerShell command and return its combined stdout+stderr output.
fn powershell(script: &str) -> Result<(String, i32), String> {
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NonInteractive",
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-Command", script,
        ])
        .output()
        .map_err(|e| format!("Impossible de lancer PowerShell : {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}{}", stdout, stderr);
    let exit_code = output.status.code().unwrap_or(-1);
    Ok((combined, exit_code))
}

// ─── SFC /scannow ─────────────────────────────────────────────────────────────
#[tauri::command]
fn run_sfc_check() -> Result<CheckResult, String> {
    let (combined, _) = powershell("sfc /scannow")?;
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
        Ok(CheckResult {
            is_ok: true,
            detail: "Aucune violation d'intégrité trouvée. Le système de fichiers est sain.".into(),
            not_found: false,
        })
    } else if found_violations {
        let _ = powershell("Start-Process -FilePath 'dism.exe' -ArgumentList '/Online','/Cleanup-Image','/RestoreHealth' -WindowStyle Hidden");
        Ok(CheckResult {
            is_ok: false,
            detail: "Des violations d'intégrité ont été détectées. Une réparation DISM /RestoreHealth a été lancée. Redémarrez l'ordinateur pour appliquer les corrections.".into(),
            not_found: false,
        })
    } else if combined.trim().is_empty() {
        Ok(CheckResult {
            is_ok: false,
            detail: "SFC nécessite des droits administrateur. Relancez Dr Reco en tant qu'administrateur.".into(),
            not_found: false,
        })
    } else {
        let preview: String = combined.chars().take(300).collect();
        Ok(CheckResult {
            is_ok: true,
            detail: format!("SFC terminé. Résultat : {}", preview.trim()),
            not_found: false,
        })
    }
}

// ─── CHKDSK C: ───────────────────────────────────────────────────────────────
#[tauri::command]
fn run_chkdsk() -> Result<CheckResult, String> {
    let (combined, exit_code) = powershell("chkdsk C:")?;
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
        Ok(CheckResult {
            is_ok: false,
            detail: "Des erreurs ont été trouvées sur le disque C:. Une vérification complète (chkdsk /f /r) a été planifiée au prochain démarrage.".into(),
            not_found: false,
        })
    } else if already_scheduled {
        Ok(CheckResult {
            is_ok: false,
            detail: "CHKDSK est déjà planifié au prochain démarrage. Redémarrez l'ordinateur pour lancer la vérification.".into(),
            not_found: false,
        })
    } else if exit_code == 0 || lower.contains("no problems found") || lower.contains("aucun problème") {
        Ok(CheckResult {
            is_ok: true,
            detail: "Le disque C: a été vérifié. Aucun problème détecté.".into(),
            not_found: false,
        })
    } else {
        Ok(CheckResult {
            is_ok: true,
            detail: format!("CHKDSK terminé (code {}). Disque en bon état.", exit_code),
            not_found: false,
        })
    }
}

// ─── Cryptolib CPS ───────────────────────────────────────────────────────────
const MIN_CRYPTOLIB_VERSION: &str = "5.2.5";

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
                    format!(
                        "Cryptolib CPS version {} — conforme (minimum requis : {}).",
                        version, MIN_CRYPTOLIB_VERSION
                    )
                } else {
                    format!(
                        "Cryptolib CPS version {} installée, mais {} minimum requis. Téléchargez la dernière version sur le site de l'ANS.",
                        version, MIN_CRYPTOLIB_VERSION
                    )
                };
                Ok(CheckResult { is_ok, detail, not_found: false })
            }
            Err(_) => Ok(CheckResult {
                is_ok: false,
                detail: "Cryptolib CPS trouvé dans le registre mais la valeur 'Version' est manquante.".into(),
                not_found: true,
            }),
        },
        Err(_) => Ok(CheckResult {
            is_ok: false,
            detail: "Cryptolib CPS n'est pas installé. Téléchargez-le depuis le portail de l'ANS (Agence du Numérique en Santé).".into(),
            not_found: true,
        }),
    }
}

fn compare_versions(a: &str, b: &str) -> i32 {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.').filter_map(|x| x.parse().ok()).collect()
    };
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
    powershell(&format!("Start-Process '{}'", url))?;
    Ok(())
}

// ─── Repair commands ──────────────────────────────────────────────────────────
// kind: "sfc"    → dism /online /cleanup-image /restorehealth
// kind: "chkdsk" → chkdsk C: /f /r  (piped Y to auto-confirm scheduling)
#[tauri::command]
fn run_repair(kind: String) -> Result<CheckResult, String> {
    let (script, label) = match kind.as_str() {
        "sfc" => (
            "dism /online /cleanup-image /restorehealth",
            "Réparation DISM",
        ),
        "chkdsk" => (
            "'Y' | chkdsk C: /f /r",
            "Réparation CHKDSK",
        ),
        other => return Err(format!("Type de réparation inconnu : {}", other)),
    };

    match powershell(script) {
        Ok((output, code)) => {
            let is_ok = code == 0;
            let detail = if is_ok {
                format!("{} terminée avec succès.", label)
            } else {
                let preview: String = output.chars().take(400).collect();
                format!(
                    "{} terminée (code {}). Résultat : {}",
                    label,
                    code,
                    preview.trim()
                )
            };
            Ok(CheckResult { is_ok, detail, not_found: false })
        }
        Err(e) => Err(format!("Impossible de lancer {} : {}", label, e)),
    }
}


// ─── Antivirus detection ──────────────────────────────────────────────────────
// Queries Win32_SecurityCenter2 via CIM — only works on Windows desktop editions.
// productState bits: the 4th hex digit indicates real-time protection status.
// 0x1000 = enabled, 0x0000 = disabled/snoozed.
// We query for products where productState has the running bit set.
#[derive(Serialize, Deserialize)]
struct AntivirusResult {
    active: Vec<String>,   // display names of running AV products
    inactive: Vec<String>, // display names of installed but inactive AV products
}

#[tauri::command]
fn check_antivirus() -> Result<AntivirusResult, String> {
    // Get all registered AV products with their state
    let script = r#"
$avList = Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct -ErrorAction SilentlyContinue
if ($null -eq $avList) { exit 1 }
foreach ($av in $avList) {
    $state = $av.productState
    # bit 12 (0x1000) in the 24-bit productState integer indicates real-time protection ON
    $rtpOn = (($state -band 0x1000) -ne 0)
    $status = if ($rtpOn) { "active" } else { "inactive" }
    Write-Output "$status|$($av.displayName)"
}
"#;
    let (output, code) = powershell(script)?;

    if code != 0 || output.trim().is_empty() {
        // SecurityCenter2 unavailable (server edition) or no AV registered
        return Ok(AntivirusResult { active: vec![], inactive: vec![] });
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

    Ok(AntivirusResult { active, inactive })
}

// Activate Microsoft Defender via PowerShell
#[tauri::command]
fn activate_defender() -> Result<CheckResult, String> {
    // Enable real-time monitoring via Set-MpPreference
    let script = "Set-MpPreference -DisableRealtimeMonitoring $false -ErrorAction Stop";
    let (output, code) = powershell(script)?;
    let is_ok = code == 0;
    let detail = if is_ok {
        "Microsoft Defender a été activé avec succès.".to_string()
    } else {
        let preview: String = output.chars().take(300).collect();
        format!("Échec de l'activation (code {}) : {}", code, preview.trim())
    };
    Ok(CheckResult { is_ok, detail, not_found: false })
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}