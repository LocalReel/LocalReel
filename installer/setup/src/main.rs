//! LocalReel per-user installer.
//!
//! Windows Installer promotes a per-user package to a machine-wide install when
//! an administrator runs it, which registers the program in HKLM while the files
//! sit in one user's profile - uninstallable for anyone else and unreachable for
//! the rest of the household. This installer does the per-user job itself:
//! everything it writes (files, shortcuts, Apps & Features entry) belongs to the
//! current user, so no elevation is involved and the result is predictable.
//!
//! One binary, two languages. The UI language follows Windows; `--lang` and
//! `LOCALREEL_LANG` override it for testing.

#![cfg_attr(not(debug_assertions), windows_subsystem = "console")]

#[cfg(windows)]
mod i18n;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    run::main()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("localreel-setup is a Windows installer");
}

#[cfg(windows)]
mod run {
    use std::fs;
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::process::{Command, ExitCode};

    use crate::i18n::{Lang, Text};
    use crate::win;

    // The window lives in its own file but is a child of this module, so it can
    // reuse the copy/uninstall helpers without making them public.
    #[path = "../gui.rs"]
    mod gui;

    const VERSION: &str = env!("LOCALREEL_VERSION");
    const BUILD_STAMP: &str = env!("LOCALREEL_BUILT");
    /// The staged product, zipped by the packaging script and embedded here.
    const PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.zip"));

    #[derive(Debug)]
    struct Options {
        lang: Lang,
        silent: bool,
        uninstall: bool,
        remove_data: bool,
        launch: bool,
        /// Wait for a running installation to exit before replacing it. The
        /// in-app updater hands over to this installer and then stops the
        /// server, so the wait is what makes the swap possible.
        wait: bool,
        /// Launch even when silent. A plain `/S` install stays quiet and does
        /// not start anything; an update wants the program back afterwards.
        force_launch: bool,
        install_dir: PathBuf,
        /// True when `/D=` asked for a folder explicitly; the window then keeps
        /// that folder instead of the one remembered from the last run.
        explicit_dir: bool,
        /// Skip the Start Menu link (the window has checkboxes for both links).
        no_start_menu: bool,
        /// Skip the desktop link.
        no_desktop: bool,
        /// Library location chosen by the user; `None` keeps the current one.
        data_dir: Option<PathBuf>,
    }

    impl Options {
        fn parse(args: impl Iterator<Item = String>, text: &Text) -> Result<Self, String> {
            let mut options = Options {
                lang: Lang::detect(),
                silent: false,
                uninstall: false,
                remove_data: false,
                launch: true,
                wait: false,
                force_launch: false,
                install_dir: win::invocation_install_dir(),
                explicit_dir: false,
                no_start_menu: false,
                no_desktop: false,
                data_dir: None,
            };
            let mut args = args.peekable();
            while let Some(argument) = args.next() {
                match argument.as_str() {
                    "/?" | "/help" | "--help" => return Err(text.help.to_string()),
                    "/S" | "/s" | "/silent" | "--silent" => options.silent = true,
                    "/uninstall" | "--uninstall" => options.uninstall = true,
                    "/removedata" | "--removedata" => options.remove_data = true,
                    "/nolaunch" | "--nolaunch" => options.launch = false,
                    "/launch" | "--launch" => options.force_launch = true,
                    "/nostartmenu" | "--nostartmenu" => options.no_start_menu = true,
                    "/nodesktop" | "--nodesktop" | "/nodesktopicon" => options.no_desktop = true,
                    value if value.starts_with("/data=") || value.starts_with("--data=") => {
                        let raw = value.split_once('=').map(|(_, value)| value).unwrap_or("");
                        if raw.trim().is_empty() {
                            return Err(text.help.to_string());
                        }
                        options.data_dir = Some(PathBuf::from(raw));
                    }
                    "--data" | "/data" => {
                        let raw = args.next().ok_or_else(|| text.help.to_string())?;
                        options.data_dir = Some(PathBuf::from(raw));
                    }
                    "/wait" | "--wait" => options.wait = true,
                    value if value.starts_with("/lang=") || value.starts_with("--lang=") => {
                        let raw = value.split_once('=').map(|(_, value)| value).unwrap_or("");
                        options.lang = Lang::parse(raw).ok_or_else(|| {
                            text.format(text.unknown_argument, &[("argument", value)])
                        })?;
                    }
                    "--lang" | "/lang" => {
                        let raw = args.next().ok_or_else(|| text.help.to_string())?;
                        options.lang = Lang::parse(&raw).ok_or_else(|| {
                            text.format(text.unknown_argument, &[("argument", &raw)])
                        })?;
                    }
                    value if value.starts_with("/D=") || value.starts_with("--dir=") => {
                        let raw = value.split_once('=').map(|(_, value)| value).unwrap_or("");
                        if raw.trim().is_empty() {
                            return Err(text.help.to_string());
                        }
                        options.install_dir = PathBuf::from(raw);
                        options.explicit_dir = true;
                    }
                    "--dir" | "/D" => {
                        let raw = args.next().ok_or_else(|| text.help.to_string())?;
                        options.install_dir = PathBuf::from(raw);
                        options.explicit_dir = true;
                    }
                    other => {
                        return Err(text.format(text.unknown_argument, &[("argument", other)]));
                    }
                }
            }
            Ok(options)
        }
    }

    /// Why the copy step did not finish.
    enum InstallFailure {
        /// Another LocalReel holds the program files.
        Busy,
        /// Anything else, already phrased for the user.
        Failed(String),
    }

    pub fn main() -> ExitCode {
        let mut lang = Lang::detect();
        // Parse once with the detected language so errors are readable, then
        // again with the language the arguments asked for.
        let options = match Options::parse(std::env::args().skip(1), lang.text()) {
            Ok(options) => options,
            Err(message) => {
                println!("{message}");
                pause(lang.text());
                return ExitCode::from(2);
            }
        };
        lang = options.lang;
        let text = lang.text();

        // Double-clicking the installer should look like an installer: a window,
        // not a console prompt. Scripted and silent runs keep the console path,
        // which is also what the in-app updater uses.
        if !options.silent && gui::available() {
            if let Some(code) = gui::run(&options, text) {
                return ExitCode::from(code as u8);
            }
        }

        if options.uninstall {
            uninstall(text, &options)
        } else {
            install(text, &options)
        }
    }

    fn install(text: &Text, options: &Options) -> ExitCode {
        let target = &options.install_dir;
        if !options.silent {
            println!("{}", text.format(text.app_title, &[("version", VERSION)]));
            println!();
            println!("{}", text.intro);
            println!("{}", text.per_user);
            println!(
                "{}",
                text.format(text.install_dir, &[("dir", &target.to_string_lossy())])
            );
            println!(
                "{}",
                text.format(
                    text.data_dir,
                    &[("dir", &win::data_dir().to_string_lossy())]
                )
            );
            if let Some(installed) = installed_version() {
                println!();
                println!(
                    "{}",
                    text.format(text.already_installed, &[("version", &installed)])
                );
            }
            println!();
            if !confirm(text.confirm, true) {
                println!("{}", text.cancelled);
                return pause_exit(text, 2);
            }
            println!();
        }

        if options.wait && win::is_running(target) {
            // The in-app updater stops the server right after starting this
            // installer; give it a couple of minutes to actually exit before
            // reporting the usual "close it and try again".
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
            while win::is_running(target) && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        }
        if win::is_running(target) {
            println!("{}", text.busy);
            return pause_exit(text, 1);
        }
        if !options.silent {
            println!("{}", text.installing);
        }

        let mut shortcuts = Vec::new();
        if !options.no_start_menu {
            shortcuts.push(win::start_menu_shortcut());
        }
        if !options.no_desktop {
            shortcuts.push(win::desktop_shortcut());
        }

        let mut quiet = |_done: usize, _total: usize| {};
        if let Err(failure) = perform_install(
            target,
            options.wait,
            &mut quiet,
            &shortcuts,
            options.data_dir.as_deref(),
        ) {
            match failure {
                InstallFailure::Busy => println!("{}", text.busy),
                InstallFailure::Failed(error) => {
                    println!("{}", text.format(text.failed, &[("error", &error)]))
                }
            }
            return pause_exit(text, 1);
        }

        println!("{}", text.format(text.done, &[("version", VERSION)]));

        let launch = options.launch
            && (options.force_launch || (!options.silent && confirm(text.launch_question, true)));
        if launch {
            launch_installed(target);
        }
        if !options.silent {
            pause(text);
        }
        ExitCode::SUCCESS
    }

    fn uninstall(text: &Text, options: &Options) -> ExitCode {
        if !options.silent {
            println!("{}", text.uninstall_title);
            println!();
            if !confirm(text.uninstall_confirm, false) {
                println!("{}", text.cancelled);
                return pause_exit(text, 2);
            }
            println!("{}", text.uninstalling);
        }

        let target = &options.install_dir;
        let outcome = perform_uninstall(
            target,
            options.remove_data,
            &recorded_shortcuts(target),
            text,
        );
        if let Some(error) = outcome.error {
            println!("{}", text.format(text.failed, &[("error", &error)]));
            return pause_exit(text, 1);
        }

        println!("{}", text.uninstall_done);
        for line in outcome.messages {
            println!("{line}");
        }
        if !options.silent {
            pause(text);
        }
        ExitCode::SUCCESS
    }

    /// What the uninstaller removed.
    struct UninstallOutcome {
        /// Fatal: the program file survived.
        error: Option<String>,
        /// User-facing lines about the library and any non-fatal failure.
        messages: Vec<String>,
    }

    /// Remove the program and, when asked, the library. Shared by the console
    /// flow and the window so both report the same thing.
    fn perform_uninstall(
        target: &Path,
        remove_data: bool,
        shortcuts: &[PathBuf],
        text: &Text,
    ) -> UninstallOutcome {
        let detached = win::detach_running_copy(target);
        for shortcut in shortcuts {
            let _ = fs::remove_file(shortcut);
        }
        if let Err(error) = win::unregister_uninstall() {
            eprintln!("could not remove the uninstall entry: {error}");
        }
        win::clear_install_record();
        remove_program_files(target);
        if let Some(temp) = detached {
            let _ = fs::remove_file(temp);
        }
        if target.join("localreel.exe").is_file() {
            return UninstallOutcome {
                error: Some("localreel.exe is still in place".to_string()),
                messages: Vec::new(),
            };
        }

        let libraries = data_candidates(target);
        let mut messages = Vec::new();
        if remove_data {
            for data in libraries {
                if !data.is_dir() {
                    continue;
                }
                match win::remove_dir_all_with_retry(&data) {
                    Ok(()) => messages
                        .push(text.format(text.data_removed, &[("dir", &data.to_string_lossy())])),
                    Err(error) => {
                        messages.push(text.format(text.failed, &[("error", &error.to_string())]))
                    }
                }
            }
        } else {
            for data in libraries {
                if data.is_dir() {
                    messages.push(text.format(text.data_kept, &[("dir", &data.to_string_lossy())]));
                }
            }
        }
        UninstallOutcome {
            error: None,
            messages,
        }
    }

    /// Every directory that may hold the library for this installation.
    ///
    /// The recorded choice wins, then the registry, then the default and the
    /// legacy `<install>\data`. Duplicates are dropped so a kept library is
    /// reported once.
    fn data_candidates(target: &Path) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(dir) = manifest_data_dir(target) {
            dirs.push(dir);
        }
        if let Some(dir) = win::recorded_data_dir() {
            dirs.push(dir);
        }
        dirs.push(win::data_dir());
        dirs.push(target.join("data"));
        dirs.retain(|dir| dir.is_absolute());
        let mut seen = std::collections::HashSet::new();
        dirs.retain(|dir| seen.insert(dir.to_string_lossy().to_lowercase()));
        dirs
    }

    /// The library path recorded by the install that is being removed.
    fn manifest_data_dir(target: &Path) -> Option<PathBuf> {
        fs::read_to_string(target.join("install.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|value| {
                value
                    .get("data_dir")
                    .and_then(|dir| dir.as_str())
                    .map(PathBuf::from)
            })
    }

    /// The program the shortcuts start.
    ///
    /// The Electron shell in `desktop\` is the desktop app. `localreel-desktop.exe`
    /// only starts the server and opens the default browser, so it is the
    /// fallback for packages that ship without a shell.
    fn launcher_for(target: &Path) -> PathBuf {
        let shell = target.join("desktop").join("LocalReel.exe");
        if shell.is_file() {
            return shell;
        }
        let launcher = target.join("localreel-desktop.exe");
        if launcher.is_file() {
            return launcher;
        }
        target.join("localreel.exe")
    }

    /// Start the installed program, preferring the desktop shell.
    fn launch_installed(target: &Path) -> bool {
        Command::new(launcher_for(target))
            .current_dir(target)
            .spawn()
            .is_ok()
    }

    /// Copy the payload into `target` and finish the per-user registration.
    ///
    /// Shared by the console flow and the window, so both do exactly the same
    /// thing and only their presentation differs.
    fn perform_install(
        target: &Path,
        wait: bool,
        progress: &mut dyn FnMut(usize, usize),
        shortcuts: &[PathBuf],
        data_dir: Option<&Path>,
    ) -> Result<(), InstallFailure> {
        if wait && win::is_running(target) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
            while win::is_running(target) && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        }
        if win::is_running(target) {
            return Err(InstallFailure::Busy);
        }
        let files = extract_payload_with_progress(target, PAYLOAD, progress)
            .map_err(InstallFailure::Failed)?;
        finish_install(target, files, shortcuts, data_dir).map_err(InstallFailure::Failed)?;
        Ok(())
    }

    /// Shortcuts that belong to the current installation.
    ///
    /// The manifest records exactly what was created; older manifests have no
    /// list, and those installs owned both links.
    fn recorded_shortcuts(target: &Path) -> Vec<PathBuf> {
        fs::read_to_string(target.join("install.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|value| {
                value
                    .get("shortcuts")
                    .and_then(|list| list.as_array())
                    .cloned()
            })
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .map(PathBuf::from)
                    .collect::<Vec<_>>()
            })
            .filter(|list| !list.is_empty())
            .unwrap_or_else(win::shortcut_paths)
    }

    /// Unpack the embedded payload.
    ///
    /// A zip made from the stage folder wraps everything in one directory; a
    /// zip made from its contents does not. Strip a wrapper only when the
    /// archive actually has one, so both layouts install correctly.
    #[cfg(test)]
    fn extract_payload(target: &Path, payload: &[u8]) -> Result<Vec<String>, String> {
        extract_payload_with_progress(target, payload, &mut |_, _| {})
    }

    /// Unpack the payload, reporting `(done, total)` entries as it goes.
    ///
    /// The counter is advisory: a skipped directory entry still advances it, so
    /// the window can show movement even when the archive has few real files.
    fn extract_payload_with_progress(
        target: &Path,
        payload: &[u8],
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<Vec<String>, String> {
        let mut files = Vec::new();
        fs::create_dir_all(target).map_err(|error| error.to_string())?;
        let reader = io::Cursor::new(payload);
        let mut archive = zip::ZipArchive::new(reader)
            .map_err(|error| format!("payload is not a zip: {error}"))?;
        let total = archive.len();
        let wrapper = wrapper_folder(&mut archive);
        for index in 0..total {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| format!("payload entry {index}: {error}"))?;
            let Some(name) = entry.enclosed_name() else {
                progress(index + 1, total);
                continue;
            };
            let mut components = name.components();
            if wrapper.is_some() {
                components.next();
            }
            let relative: PathBuf = components.collect();
            if relative.as_os_str().is_empty() {
                progress(index + 1, total);
                continue;
            }
            let destination = target.join(&relative);
            if entry.is_dir() {
                fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
                progress(index + 1, total);
                continue;
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut file = fs::File::create(&destination)
                .map_err(|error| format!("{}: {error}", destination.display()))?;
            io::copy(&mut entry, &mut file)
                .map_err(|error| format!("{}: {error}", destination.display()))?;
            files.push(relative.to_string_lossy().replace('\\', "/"));
            progress(index + 1, total);
        }
        if !files.iter().any(|file| file == "localreel.exe") {
            return Err("the payload did not contain localreel.exe".to_string());
        }
        Ok(files)
    }

    /// The single top-level directory every entry shares, if there is one.
    fn wrapper_folder(archive: &mut zip::ZipArchive<io::Cursor<&[u8]>>) -> Option<String> {
        let mut wrapper: Option<String> = None;
        for index in 0..archive.len() {
            let entry = archive.by_index(index).ok()?;
            let name = entry.enclosed_name()?;
            let mut components = name.components();
            let first = components
                .next()?
                .as_os_str()
                .to_string_lossy()
                .into_owned();
            if components.next().is_none() && !entry.is_dir() {
                // A top-level file means the archive is already flat.
                return None;
            }
            match &wrapper {
                None => wrapper = Some(first),
                Some(existing) if *existing == first => {}
                Some(_) => return None,
            }
        }
        wrapper
    }

    /// Everything after the files: manifest, uninstall entry, shortcuts.
    fn finish_install(
        target: &Path,
        mut files: Vec<String>,
        shortcuts: &[PathBuf],
        data_dir: Option<&Path>,
    ) -> Result<(), String> {
        let self_copy = target.join("localreel-setup.exe");
        if let Ok(current) = std::env::current_exe() {
            if current != self_copy {
                fs::copy(&current, &self_copy)
                    .map_err(|error| format!("cannot copy the installer: {error}"))?;
            }
        }

        // Record exactly what was written. Uninstall removes those files and
        // leaves everything else alone, so a library that the relative
        // `[data] dir` put inside the install folder survives the uninstall.
        files.push("localreel-setup.exe".to_string());
        files.sort();
        files.dedup();
        let estimated_kb = files
            .iter()
            .filter_map(|relative| safe_program_path(target, relative))
            .filter_map(|path| fs::metadata(path).ok())
            .map(|meta| meta.len())
            .sum::<u64>()
            / 1024;
        let shortcut_list: Vec<String> = shortcuts
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();
        // A chosen library is written to the server's settings before anything
        // else, so a failure here leaves no half-installed registration.
        if let Some(dir) = data_dir {
            win::set_config_data_dir(dir)
                .map_err(|error| format!("cannot update the library location: {error}"))?;
        }
        let library = data_dir
            .map(Path::to_path_buf)
            .unwrap_or_else(win::data_dir);
        let manifest = build_manifest(target, &files, &shortcut_list, estimated_kb, &library);
        fs::write(
            target.join("install.json"),
            serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("cannot write install.json: {error}"))?;

        let launcher = launcher_for(target);
        for shortcut in shortcuts {
            if let Err(error) = win::create_shortcut(shortcut, &launcher, target, "LocalReel") {
                // A missing Desktop is not worth failing the install for.
                eprintln!("shortcut {}: {error}", shortcut.display());
            }
        }

        let uninstall_string = format!("\"{}\" /uninstall", self_copy.display());
        win::register_uninstall(target, VERSION, estimated_kb, &uninstall_string)
            .map_err(|error| format!("cannot register the uninstall entry: {error}"))?;
        win::record_install(target, VERSION, &library);
        Ok(())
    }

    /// The JSON written next to the program.
    ///
    /// `shortcuts` lists only the links this run created: uninstall must not
    /// delete a link the user asked us not to make.
    fn build_manifest(
        target: &Path,
        files: &[String],
        shortcuts: &[String],
        estimated_kb: u64,
        data_dir: &Path,
    ) -> serde_json::Value {
        serde_json::json!({
            "product": "LocalReel",
            "file_manifest_version": 2,
            "version": VERSION,
            "build": BUILD_STAMP,
            "installed_at": timestamp(),
            "install_dir": target.to_string_lossy(),
            "data_dir": data_dir.to_string_lossy(),
            "per_user": true,
            "estimated_kb": estimated_kb,
            "files": files,
            "shortcuts": shortcuts,
        })
    }

    fn installed_version() -> Option<String> {
        // The product key is written by this installer; the uninstall entry
        // covers copies installed by older versions.
        win::installed_version().or_else(|| {
            let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
            let key = hkcu
                .open_subkey_with_flags(win::UNINSTALL_KEY, winreg::enums::KEY_READ)
                .ok()?;
            key.get_value::<String, _>("DisplayVersion").ok()
        })
    }

    fn timestamp() -> String {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string())
    }

    // Never traverse a symlink/junction or accept a manifest path outside the
    // installation. Old manifests inventoried user files, so only v2 is trusted.
    fn safe_program_path(target: &Path, relative: &str) -> Option<PathBuf> {
        use std::os::windows::fs::MetadataExt;
        use std::path::Component;
        if fs::symlink_metadata(target)
            .map(|meta| meta.file_attributes() & 0x400 != 0)
            .unwrap_or(false)
        {
            return None;
        }
        let relative = Path::new(relative);
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return None;
        }
        let lower = relative.to_string_lossy().replace('\\', "/").to_lowercase();
        if lower == "localreel.toml" || lower.starts_with("data/") {
            return None;
        }
        let mut path = target.to_path_buf();
        for part in relative.components() {
            path.push(part);
            if let Ok(meta) = fs::symlink_metadata(&path) {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return None;
                }
            }
        }
        Some(path)
    }

    /// Remove the files the manifest lists and any directory they leave empty.
    ///
    /// Everything else stays, which is the point: when `[data] dir` is relative
    /// the library lives under the install folder, and uninstalling the program
    /// must not take the household's media with it.
    fn remove_program_files(target: &Path) {
        use std::os::windows::fs::MetadataExt;
        if fs::symlink_metadata(target)
            .map(|meta| meta.file_attributes() & 0x400 != 0)
            .unwrap_or(true)
        {
            return;
        }
        let listed: Vec<String> = fs::read_to_string(target.join("install.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|value| {
                if value.get("file_manifest_version").and_then(|v| v.as_u64()) != Some(2) {
                    return None;
                }
                value
                    .get("files")
                    .and_then(|files| files.as_array())
                    .cloned()
            })
            .map(|files| {
                files
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();

        let listed = if listed.is_empty() {
            // Legacy manifests are polluted by user data. Prefer leftover
            // program files to data loss; never recursively delete directories.
            vec![
                "localreel.exe".into(),
                "localreel-desktop.exe".into(),
                "localreel-setup.exe".into(),
            ]
        } else {
            listed
        };
        let mut parents = Vec::new();
        for relative in listed {
            if let Some(path) = safe_program_path(target, &relative) {
                let _ = fs::remove_file(&path);
                let mut parent = path.parent();
                while let Some(dir) = parent {
                    if dir == target {
                        break;
                    }
                    parents.push(dir.to_path_buf());
                    parent = dir.parent();
                }
            }
        }
        let _ = fs::remove_file(target.join("install.json"));
        parents.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        for parent in parents {
            let _ = fs::remove_dir(parent);
        }
        let _ = fs::remove_dir(target);
    }

    fn confirm(question: &str, default_yes: bool) -> bool {
        print!("{question}");
        let _ = io::stdout().flush();
        let mut answer = String::new();
        if io::stdin().read_line(&mut answer).is_err() {
            return false;
        }
        let answer = answer.trim().to_ascii_lowercase();
        if answer.is_empty() {
            return default_yes;
        }
        matches!(answer.as_str(), "y" | "yes")
    }

    fn pause(text: &Text) {
        println!();
        print!("{}", text.press_enter);
        let _ = io::stdout().flush();
        let mut sink = String::new();
        let _ = io::stdin().read_line(&mut sink);
    }

    fn pause_exit(text: &Text, code: u8) -> ExitCode {
        pause(text);
        ExitCode::from(code)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn parse(args: &[&str]) -> Result<Options, String> {
            Options::parse(args.iter().map(|value| value.to_string()), Lang::En.text())
        }

        #[test]
        fn parses_switches() {
            let options = parse(&["/S", "/lang=zh", "/D=C:\\Apps\\LocalReel"]).unwrap();
            assert!(options.silent);
            assert_eq!(options.lang, Lang::Zh);
            assert_eq!(options.install_dir, PathBuf::from("C:\\Apps\\LocalReel"));
        }

        #[test]
        fn parses_uninstall_and_data_removal() {
            let options = parse(&["/uninstall", "/removedata"]).unwrap();
            assert!(options.uninstall);
            assert!(options.remove_data);
        }

        #[test]
        fn rejects_unknown_arguments() {
            let error = parse(&["--frobnicate"]).unwrap_err();
            assert!(error.contains("--frobnicate"));
        }

        #[test]
        fn the_manifest_lists_only_the_shortcuts_that_were_created() {
            let dir = std::env::temp_dir()
                .join(format!("localreel-manifest-test-{}", std::process::id()));
            let files = vec!["localreel.exe".to_string()];
            let start = dir.join("Start Menu").join("LocalReel.lnk");
            let value = build_manifest(
                &dir,
                &files,
                &[start.to_string_lossy().to_string()],
                42,
                &dir.join("library"),
            );
            let listed = value["shortcuts"].as_array().unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0], start.to_string_lossy().as_ref());
            assert_eq!(value["file_manifest_version"], 2);
            assert_eq!(
                value["data_dir"],
                dir.join("library").to_string_lossy().as_ref()
            );
        }

        #[test]
        fn uninstall_reads_the_shortcuts_recorded_by_the_install() {
            let dir = std::env::temp_dir()
                .join(format!("localreel-shortcuts-test-{}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("install.json"),
                serde_json::json!({
                    "file_manifest_version": 2,
                    "files": ["localreel.exe"],
                    "shortcuts": ["C:\\Users\\someone\\Desktop\\LocalReel.lnk"],
                })
                .to_string(),
            )
            .unwrap();
            let recorded = recorded_shortcuts(&dir);
            assert_eq!(
                recorded,
                vec![PathBuf::from("C:\\Users\\someone\\Desktop\\LocalReel.lnk")]
            );
            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn shortcut_switches_are_parsed() {
            let options = parse(&["/nodesktop", "/nostartmenu"]).unwrap();
            assert!(options.no_desktop);
            assert!(options.no_start_menu);
            let defaults = parse(&[]).unwrap();
            assert!(!defaults.no_desktop);
            assert!(!defaults.no_start_menu);
        }

        #[test]
        fn shortcuts_prefer_the_desktop_shell_over_the_browser_launcher() {
            let dir = std::env::temp_dir().join(format!(
                "localreel-launcher-test-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            assert_eq!(launcher_for(&dir), dir.join("localreel.exe"));

            fs::write(dir.join("localreel-desktop.exe"), b"launcher").unwrap();
            assert_eq!(launcher_for(&dir), dir.join("localreel-desktop.exe"));

            fs::create_dir_all(dir.join("desktop")).unwrap();
            fs::write(dir.join("desktop").join("LocalReel.exe"), b"shell").unwrap();
            assert_eq!(
                launcher_for(&dir),
                dir.join("desktop").join("LocalReel.exe")
            );
            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn upgrade_records_payload_only_and_preserves_custom_library() {
            let dir =
                std::env::temp_dir().join(format!("localreel-upgrade-test-{}", std::process::id()));
            assert!(
                !dir.exists(),
                "test directory already exists: {}",
                dir.display()
            );
            fs::create_dir_all(dir.join("library")).unwrap();
            fs::write(dir.join("library/media.mp4"), b"media").unwrap();
            fs::write(dir.join("library/localreel.db"), b"db").unwrap();
            fs::write(dir.join("localreel.toml"), b"[data]\ndir='library'").unwrap();
            let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
            zip.start_file("localreel.exe", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"program").unwrap();
            let payload = zip.finish().unwrap().into_inner();
            for _ in 0..2 {
                let files = extract_payload(&dir, &payload).unwrap();
                assert_eq!(files, vec!["localreel.exe"]);
                fs::write(
                    dir.join("install.json"),
                    serde_json::json!({"file_manifest_version":2,"files":files}).to_string(),
                )
                .unwrap();
            }
            remove_program_files(&dir);
            assert!(!dir.join("localreel.exe").exists());
            assert_eq!(fs::read(dir.join("library/media.mp4")).unwrap(), b"media");
            assert_eq!(fs::read(dir.join("library/localreel.db")).unwrap(), b"db");
            assert!(dir.join("localreel.toml").is_file());
            fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn polluted_legacy_manifest_and_missing_manifest_preserve_custom_files() {
            let dir =
                std::env::temp_dir().join(format!("localreel-legacy-test-{}", std::process::id()));
            assert!(!dir.exists());
            fs::create_dir_all(dir.join("library")).unwrap();
            fs::write(dir.join("library/media.mp4"), b"media").unwrap();
            fs::write(dir.join("localreel.exe"), b"program").unwrap();
            fs::write(
                dir.join("install.json"),
                br#"{"files":["localreel.exe","library/media.mp4"]}"#,
            )
            .unwrap();
            remove_program_files(&dir);
            assert!(dir.join("library/media.mp4").exists());
            remove_program_files(&dir);
            assert!(dir.join("library/media.mp4").exists());
            assert!(safe_program_path(&dir, "../outside.txt").is_none());
            fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn uninstall_keeps_a_library_inside_the_install_folder() {
            let dir =
                std::env::temp_dir().join(format!("localreel-setup-test-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join("data")).unwrap();
            fs::write(dir.join("localreel.exe"), b"exe").unwrap();
            fs::write(dir.join("localreel.toml"), b"config").unwrap();
            fs::write(dir.join("data").join("localreel.db"), b"db").unwrap();
            fs::write(
                dir.join("install.json"),
                br#"{"files":["localreel.exe","install.json"]}"#,
            )
            .unwrap();

            remove_program_files(&dir);

            assert!(!dir.join("localreel.exe").exists());
            assert!(dir.join("localreel.toml").exists());
            assert!(dir.join("data").join("localreel.db").exists());
            let _ = fs::remove_dir_all(&dir);
        }
    }
}
