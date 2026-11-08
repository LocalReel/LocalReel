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

/// The shortcuts this installer owns.
pub fn shortcut_paths() -> Vec<PathBuf> {
    vec![
        start_menu_programs_dir().join("LocalReel.lnk"),
        desktop_dir().join("LocalReel.lnk"),
    ]
}

/// Write one {@code .lnk} pointing at the launcher.
pub fn create_shortcut(link: &Path, target: &Path, working_dir: &Path, description: &str) -> io::Result<()> {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut shell_link = mslnk::ShellLink::new(target)
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))?;
    shell_link.set_name(Some(description.to_string()));
    shell_link.set_working_dir(Some(working_dir.to_string_lossy().into_owned()));
    shell_link.set_icon_location(Some(format!("{},0", target.display())));
    shell_link
        .create_lnk(link)
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))
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
    key.set_value("InstallLocation", &install_dir.to_string_lossy().to_string())?;
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
    let temp = std::env::temp_dir().join(format!(
        "localreel-setup-{}.old.exe",
        std::process::id()
    ));
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
}
