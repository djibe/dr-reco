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
    #[serde(default)]
    admin_required: bool,
}

impl CheckResult {
    fn ok(detail: impl Into<String>) -> Self {
        Self { is_ok: true, detail: detail.into(), not_found: false, ps_unavailable: false, admin_required: false }
    }
    fn err(detail: impl Into<String>) -> Self {
        Self { is_ok: false, detail: detail.into(), not_found: false, ps_unavailable: false, admin_required: false }
    }
    fn unavailable(detail: impl Into<String>) -> Self {
        Self { is_ok: false, detail: detail.into(), not_found: false, ps_unavailable: true, admin_required: false }
    }
    fn missing(detail: impl Into<String>) -> Self {
        Self { is_ok: false, detail: detail.into(), not_found: true, ps_unavailable: false, admin_required: false }
    }
}

// ─── Administrator rights ───────────────────────────────────────────────────
// The app manifest requests elevation on launch, but the binary can still run
// without it (dev mode, manifest bypass). Commands below fail with misleading
// errors when not elevated, so they check first via the native Win32 API
// (no extra crate, no PowerShell spawn).
#[link(name = "shell32")]
unsafe extern "system" {
    unsafe fn IsUserAnAdmin() -> i32;
}

fn is_elevated() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

fn admin_required() -> CheckResult {
    CheckResult { is_ok: false,
        detail: "Cette vérification nécessite des droits administrateur. Relancer Dr Reco en tant qu'administrateur.".into(),
        not_found: false, ps_unavailable: false, admin_required: true }
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
            "PowerShell est introuvable ou inaccessible : {}. Vérifier que PowerShell est installé sur l’ordinateur.",
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
        Ok(CheckResult::err("Des violations d'intégrité ont été détectées. Une réparation DISM /RestoreHealth a été lancée. Redémarrer l'ordinateur pour appliquer les corrections."))
    } else if combined.trim().is_empty() {
        Ok(CheckResult::unavailable("SFC n'a produit aucun résultat. Des droits administrateur sont peut-être nécessaires."))
    } else {
        let preview: String = combined.chars().take(300).collect();
        Ok(CheckResult::ok(format!("SFC terminé. Résultat : {}", preview.trim())))
    }
}


// ─── Disk Cleanup ─────────────────────────────────────────────────────────────
#[tauri::command]
async fn run_disk_cleanup(app: tauri::AppHandle) -> Result<CheckResult, String> {
    match app.shell()
        .command("cleanmgr")
        .args(["/D", "C", "/LOWDISK"])
        .output()
        .await
    {
        Err(e) => Ok(CheckResult::unavailable(
            format!("Impossible de lancer le nettoyage du disque — {}", e)
        )),
        Ok(output) => {
            let code = output.status.code().unwrap_or(-1);
            if code == 0 {
                Ok(CheckResult::ok("Nettoyage du disque C: effectué avec succès."))
            } else {
                Ok(CheckResult::err(format!(
                    "Le nettoyage du disque a retourné le code {}. Des droits administrateur sont peut-être nécessaires.",
                    code
                )))
            }
        }
    }
}


// ─── CHKDSK C: ───────────────────────────────────────────────────────────────
#[tauri::command]
async fn run_chkdsk(app: tauri::AppHandle) -> Result<CheckResult, String> {
    if !is_elevated() {
        return Ok(admin_required());
    }
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
        Ok(CheckResult::err("CHKDSK est déjà planifié au prochain démarrage. Redémarrer l'ordinateur pour lancer la vérification."))
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
                    format!("Cryptolib CPS version {} installée, mais {} minimum requis. Télécharger la dernière version sur le site de l'ANS.", version, MIN_CRYPTOLIB_VERSION)
                };
                Ok(CheckResult { is_ok, detail, not_found: false, ps_unavailable: false, admin_required: false })
            }
            Err(_) => Ok(CheckResult::missing("Cryptolib CPS trouvé dans le registre mais la valeur 'Version' est manquante.")),
        },
        Err(_) => Ok(CheckResult::missing("Cryptolib CPS n'est pas installé. Télécharger le composant depuis le portail de l'ANS (Agence du Numérique en Santé).")),
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
            Ok(CheckResult { is_ok, detail, not_found: false, ps_unavailable: false, admin_required: false })
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

// ─── System disk detection ───────────────────────────────────────────────────
// Shared by the storage type check and the TRIM check: both need the physical
// disk hosting the C: volume. Get-Partition gives the real disk number, which
// is then matched against the physical disk (DeviceId equals DiskNumber).
struct SystemDisk {
    media_type: String,
    spindle:    u32,
    model:      String,
}

enum MediaKind {
    Ssd,
    Scm,
    Hdd,
    Unknown,
}

impl SystemDisk {
    /// `MediaType` comes back either as a numeric code (3/4/5) or as a name
    /// ("HDD"/"SSD"/"SCM") depending on the PowerShell build, so accept both.
    fn media_kind(&self) -> MediaKind {
        match self.media_type.trim().to_lowercase().as_str() {
            "3" | "hdd" => MediaKind::Hdd,
            "4" | "ssd" => MediaKind::Ssd,
            "5" | "scm" => MediaKind::Scm,
            _ => MediaKind::Unknown,
        }
    }

    fn is_ssd(&self) -> bool {
        match self.media_kind() {
            MediaKind::Ssd | MediaKind::Scm => true,
            MediaKind::Hdd => false,
            // Unspecified media type: only a spinning disk reports a spindle speed.
            MediaKind::Unknown => self.spindle == 0,
        }
    }

    fn type_label(&self) -> &'static str {
        match self.media_kind() {
            MediaKind::Ssd => "SSD",
            MediaKind::Scm => "SSD (SCM/Optane)",
            MediaKind::Hdd => "HDD",
            MediaKind::Unknown => if self.spindle == 0 { "SSD/NVMe" } else { "HDD" },
        }
    }
}

enum DiskProbe {
    Found(SystemDisk),
    Unknown,
    Unavailable(String),
}

async fn detect_c_disk(app: &tauri::AppHandle) -> DiskProbe {
    let script = r#"
try {
    $diskNum = (Get-Partition -DriveLetter C -ErrorAction Stop).DiskNumber
    $disk = Get-PhysicalDisk -ErrorAction SilentlyContinue |
        Where-Object { $_.DeviceId -eq $diskNum } | Select-Object -First 1
    if ($null -eq $disk) {
        $d = Get-Disk -Number $diskNum -ErrorAction SilentlyContinue
        if ($null -eq $d) { Write-Output "none"; exit 0 }
        Write-Output "$($d.MediaType)|0|$($d.FriendlyName)"
    } else {
        Write-Output "$($disk.MediaType)|$($disk.SpindleSpeed)|$($disk.FriendlyName)"
    }
} catch {
    Write-Output "none"
}
"#;
    match powershell(app, script).await {
        PsResult::SpawnFailed(e) => DiskProbe::Unavailable(e),
        PsResult::Ok(output, _) => {
            let line = match output.lines().find(|l| !l.trim().is_empty()) {
                Some(line) => line.trim(),
                None => return DiskProbe::Unknown,
            };
            if line == "none" {
                return DiskProbe::Unknown;
            }
            let parts: Vec<&str> = line.splitn(3, '|').collect();
            DiskProbe::Found(SystemDisk {
                media_type: parts.first().copied().unwrap_or("0").trim().to_string(),
                spindle:    parts.get(1).copied().unwrap_or("0").trim().parse::<u32>().unwrap_or(0),
                model:      parts.get(2).copied().unwrap_or("Inconnu").trim().to_string(),
            })
        }
    }
}

// ─── SSD detection ───────────────────────────────────────────────────────────
#[tauri::command]
async fn check_storage_type(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let disk = match detect_c_disk(&app).await {
        DiskProbe::Found(disk) => disk,
        DiskProbe::Unavailable(e) => return Ok(CheckResult::unavailable(
            format!("Impossible de détecter le type de stockage — {}", e)
        )),
        // Same fallback as before: an unidentified disk is treated as an SSD.
        DiskProbe::Unknown => SystemDisk {
            media_type: "0".into(),
            spindle: 0,
            model: "Inconnu".into(),
        },
    };

    let type_label = disk.type_label();
    if disk.is_ssd() {
        Ok(CheckResult::ok(format!("Stockage de type {} détecté ({}) — Performances optimales.", type_label, disk.model)))
    } else {
        Ok(CheckResult::err(format!("Disque dur très lent en place ({}, {}). Un stockage SSD accélère grandement l'ordinateur.", type_label, disk.model)))
    }
}

// ─── TRIM optimization (SSD) ─────────────────────────────────────────────────
// TRIM tells the SSD which blocks are free. The global switch is exposed by
// fsutil (DisableDeleteNotify = 0 means TRIM is allowed); the sentence around
// it is localized, so only the ASCII prefix and the numeric value are parsed.
const TRIM_MIN_DAYS: i64 = 30;
// Result of the Windows scheduled optimization task when it is still running.
const TASK_RESULT_RUNNING: i64 = 267009;

#[derive(Serialize, Deserialize)]
struct TrimResult {
    is_ok:              bool,
    not_concerned:      bool,
    needs_trim_fix:     bool,
    needs_optimization: bool,
    needs_task_fix:     bool,
    retrim_done:        bool,
    disk_type:          String,
    last_optimization:  String,
    days_since:         i64,
    task_state:         String,
    detail:             String,
    ps_unavailable:     bool,
    admin_required:     bool,
}

impl TrimResult {
    fn base() -> Self {
        Self {
            is_ok: true, not_concerned: false,
            needs_trim_fix: false, needs_optimization: false, needs_task_fix: false,
            retrim_done: false,
            disk_type: String::new(), last_optimization: String::new(), days_since: -1,
            task_state: String::new(), detail: String::new(),
            ps_unavailable: false, admin_required: false,
        }
    }
    fn not_concerned(detail: impl Into<String>) -> Self {
        Self { not_concerned: true, detail: detail.into(), ..Self::base() }
    }
    fn unavailable(detail: impl Into<String>) -> Self {
        Self { is_ok: false, ps_unavailable: true, detail: detail.into(), ..Self::base() }
    }
}

/// Read `NTFS DisableDeleteNotify = <n>` (0 = TRIM enabled).
fn parse_trim_flag(output: &str) -> Option<u32> {
    output
        .lines()
        .map(|line| line.trim())
        .find(|line| line.starts_with("NTFS DisableDeleteNotify"))
        .and_then(|line| line.split('=').nth(1))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse().ok())
}

struct OptimizationState {
    days:        i64,
    last:        String,
    task_state:  String,
    task_result: i64,
}

/// Last optimization date, combining the Windows scheduled task and the
/// timestamp Dr Reco writes after its own retrim (a manual retrim does not
/// update the scheduled task history).
async fn probe_optimization_state(app: &tauri::AppHandle) -> Option<OptimizationState> {
    let script = r#"
$own = $null
$key = "HKCU:\Software\DrReco"
if (Test-Path $key) {
    $v = (Get-ItemProperty -Path $key -Name LastTrimDate -ErrorAction SilentlyContinue).LastTrimDate
    if ($v) { $own = [DateTimeOffset]::FromUnixTimeSeconds([int64]$v).LocalDateTime }
}
$info = Get-ScheduledTaskInfo -TaskPath '\Microsoft\Windows\Defrag\' -TaskName 'ScheduledDefrag' -ErrorAction SilentlyContinue
$task = Get-ScheduledTask -TaskPath '\Microsoft\Windows\Defrag\' -TaskName 'ScheduledDefrag' -ErrorAction SilentlyContinue
$lastTask = $null
if ($null -ne $info -and $info.LastRunTime -and $info.LastRunTime.Year -ge 2000) { $lastTask = $info.LastRunTime }
$candidates = @()
if ($null -ne $lastTask) { $candidates += $lastTask }
if ($null -ne $own) { $candidates += $own }
$last = $null
if ($candidates.Count -gt 0) { $last = $candidates | Sort-Object -Descending | Select-Object -First 1 }
$days = -1
if ($null -ne $last) { $days = [int]((Get-Date) - $last).TotalDays }
$lastStr = ""
if ($null -ne $last) { $lastStr = $last.ToString("dd/MM/yyyy", [System.Globalization.CultureInfo]::InvariantCulture) }
$state = "Missing"
if ($null -ne $task) { $state = "$($task.State)" }
$result = -1
if ($null -ne $info) { $result = [int64]$info.LastTaskResult }
Write-Output "$days|$lastStr|$state|$result"
"#;
    match powershell(app, script).await {
        PsResult::SpawnFailed(_) => None,
        PsResult::Ok(output, code) => {
            if code != 0 {
                return None;
            }
            let line = output.lines().find(|l| !l.trim().is_empty())?.trim();
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            if parts.len() < 4 {
                return None;
            }
            Some(OptimizationState {
                days:        parts[0].trim().parse().unwrap_or(-1),
                last:        parts[1].trim().to_string(),
                task_state:  parts[2].trim().to_string(),
                task_result: parts[3].trim().parse().unwrap_or(-1),
            })
        }
    }
}

/// Run a real retrim on C: and remember the date in the user's registry hive.
async fn run_volume_retrim(app: &tauri::AppHandle) -> bool {
    let script = r#"
try {
    Optimize-Volume -DriveLetter C -ReTrim -ErrorAction Stop | Out-Null
} catch {
    Write-Output "retrim_error"
    exit 2
}
try {
    if (-not (Test-Path "HKCU:\Software\DrReco")) {
        New-Item -Path "HKCU:\Software\DrReco" -Force | Out-Null
    }
    New-ItemProperty -Path "HKCU:\Software\DrReco" -Name LastTrimDate `
        -Value ([int64][DateTimeOffset]::UtcNow.ToUnixTimeSeconds()) -PropertyType QWord -Force | Out-Null
} catch { }
Write-Output "retrim_ok"
exit 0
"#;
    match powershell(app, script).await {
        PsResult::SpawnFailed(_) => false,
        PsResult::Ok(output, code) => code == 0 && output.contains("retrim_ok"),
    }
}

fn days_phrase(days: i64) -> String {
    if days <= 0 {
        "aujourd’hui".to_string()
    } else if days == 1 {
        "hier".to_string()
    } else {
        format!("il y a {} jours", days)
    }
}

#[tauri::command]
async fn check_trim_optimization(app: tauri::AppHandle) -> Result<TrimResult, String> {
    // 1. Physical disk hosting C:
    let disk = match detect_c_disk(&app).await {
        DiskProbe::Found(disk) => disk,
        DiskProbe::Unavailable(e) => return Ok(TrimResult::unavailable(
            format!("Impossible de vérifier l’optimisation TRIM — {}.", e)
        )),
        DiskProbe::Unknown => return Ok(TrimResult::not_concerned(
            "Type de disque indéterminé : l’optimisation TRIM n’est pas applicable."
        )),
    };

    // A mechanical disk never uses TRIM: skip the check entirely.
    if !disk.is_ssd() {
        return Ok(TrimResult::not_concerned(format!(
            "Disque dur mécanique ({}) : l’optimisation TRIM ne concerne que les SSD. Aucune action nécessaire.",
            disk.model
        )));
    }

    // 2. TRIM switch
    let trim_enabled = match powershell_native(&app, "fsutil behavior query DisableDeleteNotify").await {
        PsResult::SpawnFailed(e) => return Ok(TrimResult::unavailable(
            format!("Impossible de lire l’état TRIM — {}.", e)
        )),
        PsResult::Ok(output, _) => match parse_trim_flag(&output) {
            Some(flag) => flag == 0,
            None => return Ok(TrimResult::not_concerned(
                "État TRIM indéterminé sur ce système : la vérification n’est pas applicable."
            )),
        },
    };

    // 3. Optimization history
    let state = match probe_optimization_state(&app).await {
        Some(state) => state,
        None => return Ok(TrimResult::unavailable(
            "Impossible de lire l’historique d’optimisation du disque."
        )),
    };

    let mut result = TrimResult::base();
    // History fields describe the state measured *before* any retrim below.
    result.disk_type = format!("{} ({})", disk.type_label(), disk.model);
    result.days_since = state.days;
    result.last_optimization = state.last.clone();
    result.task_state = state.task_state.clone();

    // Broken scheduled task: the weekly optimization no longer runs.
    let task_broken = state.task_state.eq_ignore_ascii_case("Disabled")
        || state.task_state.eq_ignore_ascii_case("Missing")
        || (state.task_result != 0 && state.task_result != TASK_RESULT_RUNNING);
    let task_message = if state.task_state.eq_ignore_ascii_case("Missing") {
        "La tâche d’optimisation automatique de Windows est introuvable sur ce poste.".to_string()
    } else if state.task_state.eq_ignore_ascii_case("Disabled") {
        "La tâche d’optimisation automatique de Windows est désactivée.".to_string()
    } else {
        format!("La dernière optimisation planifiée par Windows a échoué (code {}).", state.task_result)
    };

    // 4. TRIM disabled: report it, never change the switch silently.
    if !trim_enabled {
        result.is_ok = false;
        result.needs_trim_fix = true;
        result.detail = "TRIM désactivé : Windows ne signale plus au SSD les blocs libres, ce qui l’use plus vite. Cliquer sur « Activer TRIM » pour corriger.".to_string();
        if task_broken {
            result.needs_task_fix = true;
            result.detail = format!("{} {}", result.detail, task_message);
        }
        return Ok(result);
    }

    // 5. Stale history: optimize now (the app runs elevated, see §9 of AGENTS.md).
    let last_label = if state.last.is_empty() {
        String::new()
    } else {
        format!("{} ({})", state.last, days_phrase(state.days))
    };
    let stale = state.days < 0 || state.days > TRIM_MIN_DAYS;

    if stale {
        if !is_elevated() {
            result.is_ok = false;
            result.needs_optimization = true;
            result.admin_required = true;
            result.detail = "TRIM actif, mais l’optimisation du SSD nécessite des droits administrateur. Relancer Dr Reco en tant qu’administrateur.".to_string();
            return Ok(result);
        }
        if run_volume_retrim(&app).await {
            result.retrim_done = true;
            result.detail = if last_label.is_empty() {
                "TRIM actif. Les blocs libres du SSD ont été optimisés pendant cette analyse.".to_string()
            } else {
                format!("TRIM actif. Les blocs libres du SSD ont été optimisés pendant cette analyse (optimisation précédente : {}).", last_label)
            };
        } else {
            result.is_ok = false;
            result.needs_optimization = true;
            result.detail = if last_label.is_empty() {
                "TRIM actif, mais aucune optimisation récente n’est enregistrée. L’optimisation n’a pas pu être lancée : cliquer sur « Optimiser le SSD » pour réessayer.".to_string()
            } else {
                format!("TRIM actif, mais la dernière optimisation remonte à {} (plus de {} jours). L’optimisation n’a pas pu être lancée : cliquer sur « Optimiser le SSD » pour réessayer.", last_label, TRIM_MIN_DAYS)
            };
        }
    } else {
        result.detail = format!("TRIM actif. Dernière optimisation : {} — rien à faire.", last_label);
    }

    if task_broken {
        result.is_ok = false;
        result.needs_task_fix = true;
        result.detail = format!("{} {}", result.detail, task_message);
    }

    Ok(result)
}

/// Repair action: re-enable TRIM for the whole system and retrim C: now.
#[tauri::command]
async fn fix_trim(app: tauri::AppHandle) -> Result<CheckResult, String> {
    if !is_elevated() {
        return Ok(admin_required());
    }
    let script = r#"
$flagOk = 0
try {
    fsutil behavior set DisableDeleteNotify 0 | Out-Null
    if ($LASTEXITCODE -eq 0) { $flagOk = 1 }
} catch { }
$retrimOk = 0
try {
    Optimize-Volume -DriveLetter C -ReTrim -ErrorAction Stop | Out-Null
    $retrimOk = 1
} catch { }
if ($flagOk -eq 1 -and $retrimOk -eq 1) {
    try {
        if (-not (Test-Path "HKCU:\Software\DrReco")) {
            New-Item -Path "HKCU:\Software\DrReco" -Force | Out-Null
        }
        New-ItemProperty -Path "HKCU:\Software\DrReco" -Name LastTrimDate `
            -Value ([int64][DateTimeOffset]::UtcNow.ToUnixTimeSeconds()) -PropertyType QWord -Force | Out-Null
    } catch { }
}
Write-Output "flag=$flagOk retrim=$retrimOk"
"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible d’optimiser le disque — {}", e)
        )),
        PsResult::Ok(output, _) => {
            let line = output
                .lines()
                .map(|l| l.trim())
                .find(|l| l.contains("flag="))
                .unwrap_or("")
                .to_string();
            let parse_num = |prefix: &str| -> i64 {
                line.split_whitespace()
                    .find(|p| p.starts_with(prefix))
                    .and_then(|p| p.trim_start_matches(prefix).parse::<i64>().ok())
                    .unwrap_or(-99)
            };
            let flag_ok = parse_num("flag=") == 1;
            let retrim_ok = parse_num("retrim=") == 1;
            if flag_ok && retrim_ok {
                Ok(CheckResult::ok("TRIM activé et blocs libres du SSD optimisés avec succès."))
            } else if flag_ok {
                Ok(CheckResult::err("TRIM activé, mais l’optimisation du SSD a échoué. Réessayer ou vérifier le disque."))
            } else {
                Ok(CheckResult::err("Impossible d’activer TRIM. Des droits administrateur sont peut-être nécessaires."))
            }
        }
    }
}

/// Repair action: re-enable and restart the Windows scheduled optimization task.
#[tauri::command]
async fn enable_optimize_task(app: tauri::AppHandle) -> Result<CheckResult, String> {
    if !is_elevated() {
        return Ok(admin_required());
    }
    let script = r#"
$path = '\Microsoft\Windows\Defrag\'
$name = 'ScheduledDefrag'
$task = Get-ScheduledTask -TaskPath $path -TaskName $name -ErrorAction Stop
if ($task.State -eq 'Disabled') { Enable-ScheduledTask -InputObject $task | Out-Null }
Start-ScheduledTask -TaskPath $path -TaskName $name
Write-Output "task_ok"
"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible de relancer l’optimisation planifiée — {}", e)
        )),
        PsResult::Ok(output, code) => {
            if code == 0 && output.contains("task_ok") {
                Ok(CheckResult::ok(
                    "Optimisation planifiée de Windows réactivée et relancée. Le SSD sera optimisé en arrière-plan."
                ))
            } else {
                let preview: String = output.chars().take(300).collect();
                Ok(CheckResult::err(format!(
                    "La réactivation de l’optimisation planifiée a échoué (code {}). Résultat : {}",
                    code, preview.trim()
                )))
            }
        }
    }
}

// ─── Windows Recovery (WinRE) ─────────────────────────────────────────────────
#[tauri::command]
async fn check_winre(app: tauri::AppHandle) -> Result<CheckResult, String> {
    if !is_elevated() {
        return Ok(admin_required());
    }
    match powershell_native(&app, "reagentc /info").await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible d'exécuter reagentc — {}", e)
        )),
        PsResult::Ok(output, _) => {
            let lower = output.to_lowercase();
            if lower.contains("enabled") {
                Ok(CheckResult::ok("Windows Recovery (WinRE) est activé et opérationnel."))
            } else if lower.contains("disabled") {
                Ok(CheckResult::err("Windows Recovery (WinRE) est désactivé. Cliquer sur 'Réparer WinRE' pour le réactiver."))
            } else if lower.contains("access") || lower.contains("administrator") || lower.contains("administrateur") {
                Ok(CheckResult::unavailable("reagentc nécessite des droits administrateur. Relancer Dr Reco en tant qu'administrateur."))
            } else {
                let preview: String = output.chars().take(300).collect();
                Ok(CheckResult::unavailable(format!("Impossible de déterminer l'état de WinRE. Résultat : {}", preview.trim())))
            }
        }
    }
}

#[tauri::command]
async fn repair_winre(app: tauri::AppHandle) -> Result<CheckResult, String> {
    if !is_elevated() {
        return Ok(admin_required());
    }
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
                Ok(CheckResult::err(format!("Windows Update a démarré mais a retourné le code {}. Vérifier l'état dans les Paramètres → Windows Update.", code)))
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

// ─── Apicrypt companion ───────────────────────────────────────────────────────
const APICRYPT_COMMAND_KEY: &str = r"compagnonApicrypt\shell\open\command";

/// Read the command line declared for the Apicrypt companion ProgID.
/// Checked in the merged HKEY_CLASSES_ROOT view first, then in the machine hive.
fn apicrypt_raw_command() -> Option<String> {
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    let key = hkcr
        .open_subkey(APICRYPT_COMMAND_KEY)
        .or_else(|_| {
            RegKey::predef(HKEY_LOCAL_MACHINE)
                .open_subkey(format!("SOFTWARE\\Classes\\{}", APICRYPT_COMMAND_KEY))
        })
        .ok()?;
    key.get_value::<String, _>("").ok()
}

/// Expand %VAR% references using the current process environment.
fn expand_env_vars(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    loop {
        let Some(start) = rest.find('%') else {
            out.push_str(rest);
            break;
        };
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..start]);
        let name = &after[..end];
        match std::env::var(name) {
            Ok(v) => out.push_str(&v),
            Err(_) => {
                out.push('%');
                out.push_str(name);
                out.push('%');
            }
        }
        rest = &after[end + 1..];
    }
    out
}

/// Extract the executable path from a registry command line
/// (e.g. `"C:\Program Files\APICEM\CompagnonApicrypt\compagnonApicrypt.exe" %1`).
fn apicrypt_exe_path(command: &str) -> Option<String> {
    let command = command.trim();
    let raw = if let Some(stripped) = command.strip_prefix('"') {
        stripped.split('"').next()?
    } else {
        // Unquoted command line: keep everything up to and including ".exe".
        let lower = command.to_lowercase();
        match lower.find(".exe") {
            Some(i) => &command[..i + 4],
            None => command,
        }
    };
    let path = expand_env_vars(raw).trim().to_string();
    if path.is_empty() { None } else { Some(path) }
}

const APICRYPT_NOT_INSTALLED: &str = "Apicrypt n'est pas installé sur ce poste.";

#[tauri::command]
fn check_apicrypt_registry() -> Result<CheckResult, String> {
    match apicrypt_raw_command() {
        None => Ok(CheckResult::missing(APICRYPT_NOT_INSTALLED)),
        Some(command) => match apicrypt_exe_path(&command) {
            None => Ok(CheckResult::missing(
                "Apicrypt est déclaré dans le registre Windows, mais le chemin du programme n'a pas pu être lu.",
            )),
            Some(path) => Ok(CheckResult::ok(format!(
                "Apicrypt est déclaré sur ce poste : {}",
                path
            ))),
        },
    }
}

#[tauri::command]
fn check_apicrypt_executable() -> Result<CheckResult, String> {
    let Some(command) = apicrypt_raw_command() else {
        return Ok(CheckResult::missing(APICRYPT_NOT_INSTALLED));
    };
    let Some(path) = apicrypt_exe_path(&command) else {
        return Ok(CheckResult::missing(
            "Apicrypt est déclaré dans le registre Windows, mais le chemin du programme n'a pas pu être lu.",
        ));
    };
    if std::path::Path::new(&path).is_file() {
        Ok(CheckResult::ok(format!(
            "Le programme Apicrypt est bien présent : {}",
            path
        )))
    } else {
        Ok(CheckResult::missing(format!(
            "Le registre référence {}, mais le fichier est introuvable. Réinstaller Apicrypt.",
            path
        )))
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
                Ok(CheckResult::err("Aucun lecteur de carte à puce détecté. Vérifier que le lecteur est branché et que ses pilotes sont installés."))
            } else {
                Ok(CheckResult::ok(format!("Lecteur de carte à puce détecté : {}.", names.join(", "))))
            }
        }
    }
}

// ─── Bluetooth adapter detection ────────────────────────────────────────────
#[tauri::command]
async fn check_bluetooth_adapter(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let script = r#"Get-PnpDevice -Class Bluetooth -PresentOnly -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Status)|$($_.Problem)|$($_.FriendlyName)" }"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible de détecter un adaptateur Bluetooth — {}", e)
        )),
        PsResult::Ok(output, _) => {
            let mut active: Vec<&str> = Vec::new();
            let mut inactive: Vec<&str> = Vec::new();
            for line in output.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let mut parts = line.splitn(3, '|');
                let status = parts.next().unwrap_or("").trim();
                let problem = parts.next().unwrap_or("").trim();
                let name = parts.next().unwrap_or("").trim();
                if name.is_empty() {
                    continue;
                }
                // A healthy device reports Status "OK" / Problem "CM_PROB_NONE";
                // a disabled or faulty one reports "Error" / "CM_PROB_DISABLED"…
                let healthy = status.eq_ignore_ascii_case("OK")
                    || problem.eq_ignore_ascii_case("CM_PROB_NONE");
                if healthy {
                    active.push(name);
                } else {
                    inactive.push(name);
                }
            }
            if !active.is_empty() {
                Ok(CheckResult::ok(format!(
                    "Adaptateur Bluetooth détecté : {}. Le partage de documents avec un smartphone Android est possible via Quick Share.",
                    active.join(", ")
                )))
            } else if !inactive.is_empty() {
                Ok(CheckResult::err(format!(
                    "Adaptateur Bluetooth détecté mais désactivé ou en erreur : {}. L'activer dans les paramètres Windows, puis utiliser Quick Share.",
                    inactive.join(", ")
                )))
            } else {
                Ok(CheckResult::missing("Aucun adaptateur Bluetooth détecté sur cet ordinateur."))
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
        "chrome"  => ("Google Chrome",   156u32),
        "firefox" => ("Mozilla Firefox", 159u32),
        "edge"    => ("Microsoft Edge",  156u32),
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
        format!("{} version {} — obsolète. La version {} ou supérieure est recommandée. Mettre à jour le navigateur.", browser_label, installed, latest_major)
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
        PsResult::Ok(_, _) => Ok(CheckResult::ok(format!("La page de mise à jour de {} a été ouverte. Suivre les instructions dans le navigateur pour terminer la mise à jour.", label))),
    }
}


// ─── System Restore point ─────────────────────────────────────────────────────
// Creates a restore point before running any repair operations.
// Requires the "System Protection" service (SRSVC) to be running and at least
// one drive to have protection enabled.  Silently succeeds on systems where
// System Restore is disabled so the rest of the checks can still proceed.
#[tauri::command]
async fn create_restore_point(app: tauri::AppHandle) -> Result<CheckResult, String> {
    if !is_elevated() {
        return Ok(admin_required());
    }
    // Checkpoint type 12 = MODIFY_SETTINGS, event type 100 = BEGIN_SYSTEM_CHANGE
    let script = r#"
$svc = Get-Service -Name SRSVC -ErrorAction SilentlyContinue
if ($null -eq $svc -or $svc.Status -ne 'Running') {
    # Try to start the service (requires admin)
    Start-Service SRSVC -ErrorAction SilentlyContinue | Out-Null
    Start-Sleep -Seconds 2
}
try {
    $cp = (Get-WmiObject -Namespace "root\default" -Class "SystemRestore" -ErrorAction Stop)
    $result = [System.Management.ManagementClass]::new("root\default", "SystemRestore", $null).InvokeMethod(
        "CreateRestorePoint",
        @("Dr Reco — vérification système", 12, 100)
    )
    if ($result -eq 0) {
        Write-Output "ok"
    } else {
        Write-Output "code=$result"
    }
} catch {
    Write-Output "error=$_"
}
"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible de créer un point de restauration — {}", e)
        )),
        PsResult::Ok(output, _) => {
            let line = output.trim();
            if line == "ok" {
                Ok(CheckResult::ok(
                    "Point de restauration Windows créé — \"Dr Reco — vérification système\"."
                ))
            } else if line.starts_with("error=") || line.starts_with("code=") {
                // System Restore may be disabled on this machine — non-blocking
                Ok(CheckResult { is_ok: false,
                    detail: format!(
                        "Point de restauration non créé (la protection du système est peut-être désactivée). Résultat : {}",
                        &line[line.find('=').map(|i| i+1).unwrap_or(0)..]
                    ),
                    not_found: false, ps_unavailable: false, admin_required: false })
            } else {
                Ok(CheckResult { is_ok: false,
                    detail: "Point de restauration non créé (la protection du système est peut-être désactivée sur ce poste).".into(),
                    not_found: false, ps_unavailable: false, admin_required: false })
            }
        }
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
    if !is_elevated() {
        return Ok(admin_required());
    }
    let script = r#"
powercfg /hibernate on
$hib = $LASTEXITCODE
powercfg /h /type reduced
$type = $LASTEXITCODE
Set-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power" -Name "HiberbootEnabled" -Value 1 -Type DWord -Force
$reg = $LASTEXITCODE
Write-Output "hib=$hib type=$type reg=$reg"
"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(format!("Impossible d'activer le démarrage rapide — {}", e))),
        PsResult::Ok(output, _) => {
            let line = output
                .lines()
                .map(|l| l.trim())
                .find(|l| l.contains("hib="))
                .unwrap_or("")
                .to_string();
            let parse_num = |prefix: &str| -> i64 {
                line.split_whitespace()
                    .find(|p| p.starts_with(prefix))
                    .and_then(|p| p.trim_start_matches(prefix).parse::<i64>().ok())
                    .unwrap_or(-99)
            };
            let hib = parse_num("hib=");
            let reduced = parse_num("type=");
            let reg = parse_num("reg=");
            if hib == 0 && reduced == 0 && reg == 0 {
                Ok(CheckResult::ok("Démarrage rapide activé avec succès. Le changement sera effectif au prochain redémarrage."))
            } else {
                Ok(CheckResult::err(format!("L'activation du démarrage rapide a échoué (hibernation : {}, fichier réduit : {}, registre : {}). Relancez l'opération.", hib, reduced, reg)))
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
    if !is_elevated() {
        return Ok(admin_required());
    }
    match powershell_native(&app, "reagentc /getrecoverysettings").await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(format!("Impossible d'exécuter reagentc — {}", e))),
        PsResult::Ok(output, code) => {
            let lower = output.to_lowercase();
            if code != 0 && (lower.contains("not recognized") || lower.contains("unknown")
                || lower.contains("n'est pas reconnu") || lower.contains("getrecoverysettings")) {
                return Ok(CheckResult { is_ok: true,
                    detail: "Récupération machine rapide non disponible sur cette version de Windows (requiert Windows 11 24H2 build 26100.4700 ou supérieur).".into(),
                    not_found: true, ps_unavailable: false, admin_required: false });
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
    if !is_elevated() {
        return Ok(admin_required());
    }
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

// ─── USB settings (WinHance power docs order) ─────────────────────────────────
// Reveal helper for settings hidden by default in Windows power options:
// Attributes 2 makes the setting visible in the power plan UI.
const USB_SUBGROUP: &str = "2a737441-1930-4402-8d77-b2bebba308a3";

fn reveal_power_setting(sub: &str, setting: &str) -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let path = format!(
        "SYSTEM\\CurrentControlSet\\Control\\Power\\PowerSettings\\{}\\{}",
        sub, setting
    );
    let key = match hklm.open_subkey_with_flags(&path, KEY_SET_VALUE) {
        Ok(key) => key,
        Err(_) => match hklm.create_subkey(&path) {
            Ok((key, _)) => key,
            Err(_) => return false,
        },
    };
    key.set_value("Attributes", &2u32).is_ok()
}

// 1. USB Hub Selective Suspend Timeout (usb-hub-selective-suspend-timeout):
//    idle delay in ms before Windows powers down USB hubs. 0 disables it so
//    card readers are never suspended.
#[tauri::command]
async fn disable_usb_hub_timeout(app: tauri::AppHandle) -> Result<CheckResult, String> {
    if !is_elevated() {
        return Ok(admin_required());
    }
    // Reveal the setting in Windows power options (Attributes 2 = visible).
    let revealed = reveal_power_setting(
        USB_SUBGROUP,
        "0853a681-27c8-4100-a2fd-82013e970683",
    );
    let script = r#"
$sub = "2a737441-1930-4402-8d77-b2bebba308a3"
$set = "0853a681-27c8-4100-a2fd-82013e970683"
powercfg /SETACVALUEINDEX SCHEME_CURRENT $sub $set 0 2>&1
$acSet = $LASTEXITCODE
powercfg /SETDCVALUEINDEX SCHEME_CURRENT $sub $set 0 2>&1
$dcSet = $LASTEXITCODE
powercfg /SETACTIVE SCHEME_CURRENT 2>&1
$activated = $LASTEXITCODE
Write-Output "acSet=$acSet dcSet=$dcSet activated=$activated"
"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(format!("Impossible de modifier le plan d'alimentation — {}", e))),
        PsResult::Ok(output, _) => {
            let line = output
                .lines()
                .map(|l| l.trim())
                .find(|l| l.contains("acSet="))
                .unwrap_or("")
                .to_string();
            let parse_num = |prefix: &str| -> i64 {
                line.split_whitespace()
                    .find(|p| p.starts_with(prefix))
                    .and_then(|p| p.trim_start_matches(prefix).parse::<i64>().ok())
                    .unwrap_or(-99)
            };
            let ac_set = parse_num("acSet=");
            let dc_set = parse_num("dcSet=");
            let activated = parse_num("activated=");
            if revealed && ac_set == 0 && dc_set == 0 && activated == 0 {
                Ok(CheckResult::ok("Délai de mise en veille des hubs USB désactivé (0 ms). Les hubs USB resteront alimentés en permanence."))
            } else {
                Ok(CheckResult::err(format!("La désactivation n'a pas pu être appliquée entièrement (registre : {}, écriture secteur/batterie : {}/{}, activation : {}). Relancez l'opération.", if revealed { "OK" } else { "échec" }, ac_set, dc_set, activated)))
            }
        }
    }
}

// 2. USB selective suspend setting (usb-selective-suspend): allows Windows to
//    power down individual USB ports when idle. 0 = Disabled.
#[tauri::command]
async fn disable_usb_suspend(app: tauri::AppHandle) -> Result<CheckResult, String> {
    if !is_elevated() {
        return Ok(admin_required());
    }
    // Reveal the setting in Windows power options (Attributes 2 = visible).
    let revealed = reveal_power_setting(
        USB_SUBGROUP,
        "48e6b7a6-50f5-4782-a5d4-53bb8f07e226",
    );
    let script = r#"
$sub = "2a737441-1930-4402-8d77-b2bebba308a3"
$set = "48e6b7a6-50f5-4782-a5d4-53bb8f07e226"
powercfg /SETACVALUEINDEX SCHEME_CURRENT $sub $set 0 2>&1
$acSet = $LASTEXITCODE
powercfg /SETDCVALUEINDEX SCHEME_CURRENT $sub $set 0 2>&1
$dcSet = $LASTEXITCODE
powercfg /SETACTIVE SCHEME_CURRENT 2>&1
$activated = $LASTEXITCODE
Write-Output "acSet=$acSet dcSet=$dcSet activated=$activated"
"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(format!("Impossible de modifier le plan d'alimentation — {}", e))),
        PsResult::Ok(output, _) => {
            let line = output
                .lines()
                .map(|l| l.trim())
                .find(|l| l.contains("acSet="))
                .unwrap_or("")
                .to_string();
            let parse_num = |prefix: &str| -> i64 {
                line.split_whitespace()
                    .find(|p| p.starts_with(prefix))
                    .and_then(|p| p.trim_start_matches(prefix).parse::<i64>().ok())
                    .unwrap_or(-99)
            };
            let ac_set = parse_num("acSet=");
            let dc_set = parse_num("dcSet=");
            let activated = parse_num("activated=");
            if revealed && ac_set == 0 && dc_set == 0 && activated == 0 {
                Ok(CheckResult::ok("Mise en veille sélective USB désactivée. Les périphériques USB resteront alimentés en permanence."))
            } else {
                Ok(CheckResult::err(format!("La désactivation n'a pas pu être appliquée entièrement (registre : {}, écriture secteur/batterie : {}/{}, activation : {}). Relancez l'opération.", if revealed { "OK" } else { "échec" }, ac_set, dc_set, activated)))
            }
        }
    }
}

// 3. USB 3 Link Power Management (usb3-link-power-management): 0 = Off, so
//    Windows never puts USB 3 links into U1/U2 low-power states.
#[tauri::command]
async fn disable_usb3_lpm(app: tauri::AppHandle) -> Result<CheckResult, String> {
    if !is_elevated() {
        return Ok(admin_required());
    }
    // Reveal the setting in Windows power options (Attributes 2 = visible).
    let revealed = reveal_power_setting(
        USB_SUBGROUP,
        "d4e98f31-5ffe-4ce1-be31-1b38b384c009",
    );
    let script = r#"
$sub = "2a737441-1930-4402-8d77-b2bebba308a3"
$set = "d4e98f31-5ffe-4ce1-be31-1b38b384c009"
powercfg /SETACVALUEINDEX SCHEME_CURRENT $sub $set 0 2>&1
$acSet = $LASTEXITCODE
powercfg /SETDCVALUEINDEX SCHEME_CURRENT $sub $set 0 2>&1
$dcSet = $LASTEXITCODE
powercfg /SETACTIVE SCHEME_CURRENT 2>&1
$activated = $LASTEXITCODE
Write-Output "acSet=$acSet dcSet=$dcSet activated=$activated"
"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(format!("Impossible de modifier le plan d'alimentation — {}", e))),
        PsResult::Ok(output, _) => {
            let line = output
                .lines()
                .map(|l| l.trim())
                .find(|l| l.contains("acSet="))
                .unwrap_or("")
                .to_string();
            let parse_num = |prefix: &str| -> i64 {
                line.split_whitespace()
                    .find(|p| p.starts_with(prefix))
                    .and_then(|p| p.trim_start_matches(prefix).parse::<i64>().ok())
                    .unwrap_or(-99)
            };
            let ac_set = parse_num("acSet=");
            let dc_set = parse_num("dcSet=");
            let activated = parse_num("activated=");
            if revealed && ac_set == 0 && dc_set == 0 && activated == 0 {
                Ok(CheckResult::ok("Gestion de l'alimentation de la liaison USB 3 désactivée (Off). Les périphériques USB 3 resteront alimentés en permanence."))
            } else {
                Ok(CheckResult::err(format!("La désactivation n'a pas pu être appliquée entièrement (registre : {}, écriture secteur/batterie : {}/{}, activation : {}). Relancez l'opération.", if revealed { "OK" } else { "échec" }, ac_set, dc_set, activated)))
            }
        }
    }
}

// ─── DISM Component Cleanup ───────────────────────────────────────────────────
#[tauri::command]
async fn run_dism_cleanup(app: tauri::AppHandle) -> Result<CheckResult, String> {
    if !is_elevated() {
        return Ok(admin_required());
    }
    match app.shell()
        .command("Dism.exe")
        .args(["/online", "/Cleanup-Image", "/StartComponentCleanup", "/ResetBase"])
        .output()
        .await
    {
        Err(e) => Ok(CheckResult::unavailable(
            format!("Impossible de lancer DISM — {}", e)
        )),
        Ok(output) => {
            let code = output.status.code().unwrap_or(-1);
            if code == 0 {
                Ok(CheckResult::ok(
                    "Nettoyage des composants Windows effectué avec succès (DISM /StartComponentCleanup /ResetBase)."
                ))
            } else {
                Ok(CheckResult::err(format!(
                    "DISM a retourné le code {}. Des droits administrateur sont peut-être nécessaires.",
                    code
                )))
            }
        }
    }
}

// ─── CompactOS ────────────────────────────────────────────────────────────────
#[tauri::command]
async fn run_compact_os(app: tauri::AppHandle) -> Result<CheckResult, String> {
    if !is_elevated() {
        return Ok(admin_required());
    }
    match app.shell()
        .command("Compact.exe")
        .args(["/CompactOS:always"])
        .output()
        .await
    {
        Err(e) => Ok(CheckResult::unavailable(
            format!("Impossible de lancer Compact.exe — {}", e)
        )),
        Ok(output) => {
            let code = output.status.code().unwrap_or(-1);
            if code == 0 {
                Ok(CheckResult::ok(
                    "Compression du système d'exploitation activée avec succès (CompactOS)."
                ))
            } else {
                Ok(CheckResult::err(format!(
                    "Compact.exe a retourné le code {}. Des droits administrateur sont peut-être nécessaires.",
                    code
                )))
            }
        }
    }
}

// ─── Smart Card service (SCardSvr) ────────────────────────────────────────────
// The CPS card reader relied on by Amelipro requires the Windows Smart Card
// service to start automatically, otherwise the reader may not be detected.
#[tauri::command]
async fn check_scard_service(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let script = r#"
$svc = Get-Service -Name SCardSvr -ErrorAction SilentlyContinue
if ($null -eq $svc) { Write-Output "missing" }
else { Write-Output "$($svc.StartType)|$($svc.Status)" }
"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible de vérifier le service Carte à puce — {}", e)
        )),
        PsResult::Ok(output, _) => {
            let line = output.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
            if line == "missing" || line.is_empty() {
                return Ok(CheckResult::missing(
                    "Service Carte à puce (SCardSvr) introuvable sur cet ordinateur. Le lecteur de carte vitale risque de ne pas fonctionner."
                ));
            }
            let parts: Vec<&str> = line.splitn(2, '|').collect();
            let start_type = parts.first().copied().unwrap_or("").trim();
            let status = parts.get(1).copied().unwrap_or("").trim();
            if start_type.eq_ignore_ascii_case("Automatic") {
                Ok(CheckResult::ok(format!(
                    "Service Carte à puce (SCardSvr) en démarrage automatique (état : {}). Le lecteur de carte sera détecté au branchement.",
                    status
                )))
            } else {
                Ok(CheckResult::err(format!(
                    "Service Carte à puce (SCardSvr) en démarrage {} (état : {}). Le lecteur de carte risque de ne pas être détecté. Cliquer sur « Activer le démarrage automatique » pour corriger.",
                    start_type, status
                )))
            }
        }
    }
}

#[tauri::command]
async fn enable_scard_autostart(app: tauri::AppHandle) -> Result<CheckResult, String> {
    let script = r#"
Set-Service -Name SCardSvr -StartupType Automatic -ErrorAction Stop
Start-Service -Name SCardSvr -ErrorAction SilentlyContinue
"#;
    match powershell(&app, script).await {
        PsResult::SpawnFailed(e) => Ok(CheckResult::unavailable(
            format!("Impossible de configurer le service Carte à puce — {}", e)
        )),
        PsResult::Ok(output, code) => {
            if code == 0 {
                Ok(CheckResult::ok(
                    "Service Carte à puce (SCardSvr) configuré en démarrage automatique et démarré avec succès."
                ))
            } else {
                let preview: String = output.chars().take(300).collect();
                Ok(CheckResult::err(format!(
                    "La configuration du service a échoué (code {}). Des droits administrateur sont peut-être nécessaires. Résultat : {}",
                    code, preview.trim()
                )))
            }
        }
    }
}

// ─── Entry point ──────────────────────────────────────────────────────────────
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_hwinfo::init())
        .invoke_handler(tauri::generate_handler![
            run_sfc_check,
            run_disk_cleanup,
            run_chkdsk,
            check_cryptolib_version,
            open_url,
            run_repair,
            check_antivirus,
            activate_defender,
            check_storage_type,
            check_trim_optimization,
            fix_trim,
            enable_optimize_task,
            check_winre,
            repair_winre,
            launch_windows_update,
            check_services_cnam,
            check_apicrypt_registry,
            check_apicrypt_executable,
            check_smartcard_reader,
            check_bluetooth_adapter,
            check_scard_service,
            enable_scard_autostart,
            check_browser_and_extension,
            check_browser_version,
            launch_browser_update,
            create_restore_point,
            check_fast_startup,
            enable_fast_startup,
            check_battery_health,
            check_qmr,
            enable_qmr,
            disable_usb_suspend,
            disable_usb3_lpm,
            disable_usb_hub_timeout,
            run_dism_cleanup,
            run_compact_os,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
