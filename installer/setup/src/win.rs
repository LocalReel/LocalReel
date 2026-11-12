//! Windows integration: known folders, shortcuts, the uninstall entry and the
//! "is the application running" check.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
use winreg::RegKey;

/// Fixed identity of the per-user installation. The uninstall entry lives under
/// this key; changing it would leave two entries behind on upgrade.
pub const UNINSTALL_KEY: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{6E7B1C2A-9C41-4E3B-8A55-0B0A0F6C1D21}";

/// Where the window remembers the folder the user picked last time.
const SETUP_STATE_KEY: &str = "Software\\LocalReel\\Setup";

/// The product's own registry key: installation path, version and library.
const APP_KEY: &str = "Software\\LocalReel";

/// Publish where this copy lives, so upgrades and the uninstaller can find it
/// even when the install folder was changed.
pub fn record_install(install_dir: &Path, version: &str, data_dir: &Path) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok((key, _)) = hkcu.create_subkey(APP_KEY) {
        let _ = key.set_value("InstallPath", &install_dir.to_string_lossy().to_string());
        let _ = key.set_value("Version", &version.to_string());
        let _ = key.set_value("DataDir", &data_dir.to_string_lossy().to_string());
    }
}

/// Remove the product key on uninstall; the Apps & Features entry goes with it.
pub fn clear_install_record() {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(APP_KEY);
}

/// The installed folder, preferring the product key and falling back to the
/// uninstall entry written by older versions.
pub fn installed_dir() -> Option<PathBuf> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey_with_flags(APP_KEY, KEY_READ) {
        if let Ok(value) = key.get_value::<String, _>("InstallPath") {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                return Some(path);
            }
        }
    }
    if let Ok(key) = hkcu.open_subkey_with_flags(UNINSTALL_KEY, KEY_READ) {
        if let Ok(value) = key.get_value::<String, _>("InstallLocation") {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                return Some(path);
            }
        }
    }
    None
}

/// The version recorded by the last install, if any.
pub fn installed_version() -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey_with_flags(APP_KEY, KEY_READ).ok()?;
    key.get_value("Version").ok()
}

/// Where the library was put by the last install, if it was customised.
pub fn recorded_data_dir() -> Option<PathBuf> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey_with_flags(APP_KEY, KEY_READ).ok()?;
    let value: String = key.get_value("DataDir").ok()?;
    let path = PathBuf::from(value);
    path.is_absolute().then_some(path)
}

/// The server's settings file: `%APPDATA%\LocalReel\localreel.toml`.
pub fn config_path() -> PathBuf {
    let roaming = match std::env::var_os("APPDATA") {
        Some(value) => PathBuf::from(value),
        None => home().join("AppData").join("Roaming"),
    };
    roaming.join("LocalReel").join("localreel.toml")
}

/// The `[data] dir` the settings file names, if any.
///
/// A relative value is anchored to the program's own folder, which is what the
/// server does (`resolve_paths`), not to the settings file.
pub fn config_data_dir(install_dir: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(config_path()).ok()?;
    let value = data_dir_in(&text)?;
    let path = PathBuf::from(value);
    if path.is_absolute() {
        return Some(path);
    }
    Some(install_dir.join(path))
}

/// Point `[data] dir` at `dir`, editing only that line.
///
/// A parse-and-rewrite would drop the user's comments and ordering, so the file
/// is patched line by line and every other setting is left alone.
pub fn set_config_data_dir(dir: &Path) -> io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = fs::read_to_string(&path).unwrap_or_default();
    fs::write(&path, replace_data_dir(&text, &dir.to_string_lossy()))
}

/// Value of `[data] dir` in TOML text, without touching the filesystem.
fn data_dir_in(text: &str) -> Option<String> {
    let mut in_data = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_data = trimmed.trim_start_matches('[').trim_end_matches(']').trim() == "data";
            continue;
        }
        if !in_data {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() != "dir" {
            continue;
        }
        if let Some(parsed) = parse_toml_string(value) {
            return Some(parsed);
        }
    }
    None
}

/// Rewrite (or add) the `dir` key of `[data]`, keeping everything else.
fn replace_data_dir(text: &str, dir: &str) -> String {
    let quoted = format!("dir = \"{}\"", escape_toml_string(dir));
    let lines: Vec<&str> = text.lines().collect();
    let mut output: Vec<String> = Vec::with_capacity(lines.len() + 2);
    let mut in_data = false;
    let mut section_seen = false;
    let mut written = false;

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            // Leaving a `[data]` section without a `dir` key: insert it first.
            if in_data && !written {
                output.push(quoted.clone());
                written = true;
            }
            in_data = trimmed.trim_start_matches('[').trim_end_matches(']').trim() == "data";
            section_seen |= in_data;
        } else if in_data {
            if let Some((key, _)) = trimmed.split_once('=') {
                if key.trim() == "dir" {
                    output.push(quoted.clone());
                    written = true;
                    continue;
                }
            }
        }
        output.push((*line).to_string());
    }
    if in_data && !written {
        output.push(quoted.clone());
    }
    if !section_seen {
        if !output.is_empty() {
            output.push(String::new());
        }
        output.push("[data]".to_string());
        output.push(quoted);
    }
    let mut joined = output.join("\n");
    joined.push('\n');
    joined
}

fn escape_toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn parse_toml_string(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(ch) = chars.next() {
            match ch {
                '"' => return Some(out),
                '\\' => match chars.next()? {
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    other => out.push(other),
                },
                other => out.push(other),
            }
        }
        return None;
    }
    if let Some(rest) = raw.strip_prefix('\'') {
        return rest.split('\'').next().map(str::to_string);
    }
    None
}

/// The folder a previous run installed to, if the user picked one.
pub fn last_install_dir() -> Option<PathBuf> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey_with_flags(SETUP_STATE_KEY, KEY_READ)
        .ok()?;
    let value: String = key.get_value("InstallDir").ok()?;
    let path = PathBuf::from(value);
    path.is_absolute().then_some(path)
}

/// Remember a successful install folder; failure is not worth interrupting.
pub fn remember_install_dir(dir: &Path) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok((key, _)) = hkcu.create_subkey(SETUP_STATE_KEY) {
        let _ = key.set_value("InstallDir", &dir.to_string_lossy().to_string());
    }
}

/// Environment variable holding the user's AppData\Local.
pub fn local_app_data() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(value) => PathBuf::from(value),
        None => home().join("AppData").join("Local"),
    }
}

pub fn home() -> PathBuf {
    match std::env::var_os("USERPROFILE") {
        Some(value) => PathBuf::from(value),
        None => PathBuf::from("C:\\Users\\Default"),
    }
}

/// Where the program goes. Per-user applications live under `Programs` so the
/// install folder never doubles as the data folder.
pub fn default_install_dir() -> PathBuf {
    local_app_data().join("Programs").join("LocalReel")
}

/// The directory this installer should act on when `--dir` was not given.
///
/// The copy cached in the install folder is what Apps & Features runs, so when
/// `localreel.exe` sits next to it that folder is the target. Running the
/// downloaded installer instead falls back to the default location.
pub fn invocation_install_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir.join("localreel.exe").is_file() {
                return dir.to_path_buf();
            }
        }
    }
    default_install_dir()
}

/// Where the library lives; the installer never writes here.
pub fn data_dir() -> PathBuf {
    local_app_data().join("LocalReel")
}

/// A known folder, honouring redirection (Desktop and Start Menu are redirected
/// on plenty of machines; `%USERPROFILE%\Desktop` would be wrong there).
fn known_folder(name: &str, fallback: impl FnOnce() -> PathBuf) -> PathBuf {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey_with_flags(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders",
        KEY_READ,
    );
    if let Ok(key) = key {
        if let Ok(raw) = key.get_value::<String, _>(name) {
            let expanded = expand_environment(&raw);
            if !expanded.is_empty() {
                return PathBuf::from(expanded);
            }
        }
    }
    fallback()
}

/// Expand `%VAR%` references from an `REG_EXPAND_SZ` value.
fn expand_environment(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            result.push('%');
            result.push_str(after);
            return result;
        };
        let name = &after[..end];
        match std::env::var(name) {
            Ok(value) => result.push_str(&value),
            Err(_) => {
                result.push('%');
                result.push_str(name);
                result.push('%');
            }
        }
        rest = &after[end + 1..];
    }
    result.push_str(rest);
    result
}

pub fn desktop_dir() -> PathBuf {
    known_folder("Desktop", || home().join("Desktop"))
}

pub fn start_menu_programs_dir() -> PathBuf {
    known_folder("Programs", || {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join("AppData").join("Roaming"))
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
    })
}

pub fn start_menu_shortcut() -> PathBuf {
    start_menu_programs_dir().join("LocalReel.lnk")
}

pub fn desktop_shortcut() -> PathBuf {
    desktop_dir().join("LocalReel.lnk")
}

/// Both shortcuts, in the order the manifest lists them.
pub fn shortcut_paths() -> Vec<PathBuf> {
    vec![start_menu_shortcut(), desktop_shortcut()]
}

/// Write one {@code .lnk} pointing at the launcher.
pub fn create_shortcut(
    link: &Path,
    target: &Path,
    working_dir: &Path,
    description: &str,
) -> io::Result<()> {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut shell_link =
        mslnk::ShellLink::new(target).map_err(|error| io::Error::other(error.to_string()))?;
    shell_link.set_name(Some(description.to_string()));
    shell_link.set_working_dir(Some(working_dir.to_string_lossy().into_owned()));
    shell_link.set_icon_location(Some(format!("{},0", target.display())));
    shell_link
        .create_lnk(link)
        .map_err(|error| io::Error::other(error.to_string()))
}

/// Register the program in Apps & Features for the current user.
pub fn register_uninstall(
    install_dir: &Path,
    version: &str,
    estimated_kb: u64,
    uninstall_string: &str,
) -> io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(UNINSTALL_KEY)?;
    key.set_value("DisplayName", &"LocalReel")?;
    key.set_value("DisplayVersion", &version)?;
    key.set_value("Publisher", &"LocalReel")?;
    key.set_value(
        "DisplayIcon",
        &format!("{},0", install_dir.join("localreel.exe").display()),
    )?;
    key.set_value(
        "InstallLocation",
        &install_dir.to_string_lossy().to_string(),
    )?;
    key.set_value("UninstallString", &uninstall_string)?;
    key.set_value("NoModify", &1u32)?;
    key.set_value("NoRepair", &1u32)?;
    key.set_value("EstimatedSize", &estimated_kb)?;
    Ok(())
}

pub fn unregister_uninstall() -> io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.delete_subkey_all(UNINSTALL_KEY) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Whether the installed program is running: Windows will not let us replace an
/// executable that is open for execution, so the write test is the check.
pub fn is_running(install_dir: &Path) -> bool {
    let exe = install_dir.join("localreel.exe");
    if !exe.is_file() {
        return false;
    }
    match fs::OpenOptions::new().write(true).open(&exe) {
        Ok(_) => false,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(_) => true,
    }
}

/// Best-effort removal of a running executable: rename it out of the install
/// folder first, then let the temporary copy go when the process exits.
pub fn detach_running_copy(install_dir: &Path) -> Option<PathBuf> {
    let setup = install_dir.join("localreel-setup.exe");
    if !setup.is_file() {
        return None;
    }
    let temp = std::env::temp_dir().join(format!("localreel-setup-{}.old.exe", std::process::id()));
    match fs::rename(&setup, &temp) {
        Ok(()) => Some(temp),
        Err(_) => None,
    }
}

pub fn remove_dir_all_with_retry(path: &Path) -> io::Result<()> {
    for attempt in 0..5 {
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                if attempt == 4 {
                    return Err(error);
                }
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_environment_references() {
        std::env::set_var("LOCALREEL_TEST_DIR", "D:\\Example");
        assert_eq!(
            expand_environment("%LOCALREEL_TEST_DIR%\\Desktop"),
            "D:\\Example\\Desktop"
        );
        assert_eq!(expand_environment("no placeholders"), "no placeholders");
        assert_eq!(expand_environment("%NOPE%\\x"), "%NOPE%\\x");
    }

    #[test]
    fn rewrites_only_the_data_dir_line() {
        let text = "# my settings\n[server]\nport = 3101\n\n[data]\n# library\ndir = \"D:\\\\Old\"\nkeep = true\n";
        let updated = replace_data_dir(text, "E:\\Videos");
        assert!(updated.contains("[server]\nport = 3101"));
        assert!(updated.contains("# library"));
        assert!(updated.contains("keep = true"));
        assert!(updated.contains("dir = \"E:\\\\Videos\""));
        assert!(!updated.contains("Old"));
        assert_eq!(data_dir_in(&updated).as_deref(), Some("E:\\Videos"));
    }

    #[test]
    fn adds_the_key_to_an_existing_data_section() {
        let updated = replace_data_dir("[data]\n", "D:\\Media");
        assert_eq!(data_dir_in(&updated).as_deref(), Some("D:\\Media"));
    }

    #[test]
    fn appends_a_section_when_missing() {
        let updated = replace_data_dir("[server]\nport = 1\n", "D:\\Media");
        assert!(updated.contains("[server]"));
        assert_eq!(data_dir_in(&updated).as_deref(), Some("D:\\Media"));
    }

    #[test]
    fn reads_a_quoted_windows_path() {
        let text = "[data]\ndir = 'C:\\Users\\me\\Videos'\n";
        assert_eq!(data_dir_in(text).as_deref(), Some("C:\\Users\\me\\Videos"));
    }
}
