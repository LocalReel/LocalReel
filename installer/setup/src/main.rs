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
        install_dir: PathBuf,
    }

    impl Options {
        fn parse(args: impl Iterator<Item = String>, text: &Text) -> Result<Self, String> {
            let mut options = Options {
                lang: Lang::detect(),
                silent: false,
                uninstall: false,
                remove_data: false,
                launch: true,
                install_dir: win::invocation_install_dir(),
            };
            let mut args = args.peekable();
            while let Some(argument) = args.next() {
                match argument.as_str() {
                    "/?" | "/help" | "--help" => return Err(text.help.to_string()),
                    "/S" | "/s" | "/silent" | "--silent" => options.silent = true,
                    "/uninstall" | "--uninstall" => options.uninstall = true,
                    "/removedata" | "--removedata" => options.remove_data = true,
                    "/nolaunch" | "--nolaunch" => options.launch = false,
                    value if value.starts_with("/lang=") || value.starts_with("--lang=") => {
                        let raw = value.split_once('=').map(|(_, value)| value).unwrap_or("");
                        options.lang = Lang::parse(raw)
                            .ok_or_else(|| text.format(text.unknown_argument, &[("argument", value)]))?;
                    }
                    "--lang" | "/lang" => {
                        let raw = args.next().ok_or_else(|| text.help.to_string())?;
                        options.lang = Lang::parse(&raw)
                            .ok_or_else(|| text.format(text.unknown_argument, &[("argument", &raw)]))?;
                    }
                    value if value.starts_with("/D=") || value.starts_with("--dir=") => {
                        let raw = value.split_once('=').map(|(_, value)| value).unwrap_or("");
                        if raw.trim().is_empty() {
                            return Err(text.help.to_string());
                        }
                        options.install_dir = PathBuf::from(raw);
                    }
                    "--dir" | "/D" => {
                        let raw = args.next().ok_or_else(|| text.help.to_string())?;
                        options.install_dir = PathBuf::from(raw);
                    }
                    other => {
                        return Err(text.format(text.unknown_argument, &[("argument", other)]));
                    }
                }
            }
            Ok(options)
        }
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

        if options.uninstall {
            uninstall(text, &options)
        } else {
            install(text, &options)
        }
    }

    fn install(text: &Text, options: &Options) -> ExitCode {
        let target = &options.install_dir;
        if !options.silent {
            println!(
                "{}",
                text.format(text.app_title, &[("version", VERSION)])
            );
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

        if win::is_running(target) {
            println!("{}", text.busy);
            return pause_exit(text, 1);
        }
        if !options.silent {
            println!("{}", text.installing);
        }

        if let Err(error) = extract(target) {
            println!("{}", text.format(text.failed, &[("error", &error)]));
            return pause_exit(text, 1);
        }
        if let Err(error) = finish_install(target) {
            println!("{}", text.format(text.failed, &[("error", &error)]));
            return pause_exit(text, 1);
        }

        println!(
            "{}",
            text.format(text.done, &[("version", VERSION)])
        );

        if options.launch && !options.silent && confirm(text.launch_question, true) {
            let launcher = target.join("localreel-desktop.exe");
            let launcher = if launcher.is_file() {
                launcher
            } else {
                target.join("localreel.exe")
            };
            let _ = Command::new(launcher).current_dir(target).spawn();
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
        let detached = win::detach_running_copy(target);
        for shortcut in win::shortcut_paths() {
            let _ = fs::remove_file(shortcut);
        }
        if let Err(error) = win::unregister_uninstall() {
            eprintln!("could not remove the uninstall entry: {error}");
        }
        if let Err(error) = win::remove_dir_all_with_retry(target) {
            println!("{}", text.format(text.failed, &[("error", &error.to_string())]));
            return pause_exit(text, 1);
        }
        if let Some(temp) = detached {
            let _ = fs::remove_file(temp);
        }

        println!("{}", text.uninstall_done);
        let data = win::data_dir();
        if options.remove_data {
            match win::remove_dir_all_with_retry(&data) {
                Ok(()) => println!("{}", text.format(text.data_removed, &[("dir", &data.to_string_lossy())])),
                Err(error) => println!(
                    "{}",
                    text.format(text.failed, &[("error", &error.to_string())])
                ),
            }
        } else if data.is_dir() {
            println!(
                "{}",
                text.format(text.data_kept, &[("dir", &data.to_string_lossy())])
            );
        }
        if !options.silent {
            pause(text);
        }
        ExitCode::SUCCESS
    }

    /// Unpack the embedded payload.
    ///
    /// A zip made from the stage folder wraps everything in one directory; a
    /// zip made from its contents does not. Strip a wrapper only when the
    /// archive actually has one, so both layouts install correctly.
    fn extract(target: &Path) -> Result<(), String> {
        fs::create_dir_all(target).map_err(|error| error.to_string())?;
        let reader = io::Cursor::new(PAYLOAD);
        let mut archive =
            zip::ZipArchive::new(reader).map_err(|error| format!("payload is not a zip: {error}"))?;
        let wrapper = wrapper_folder(&mut archive);
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| format!("payload entry {index}: {error}"))?;
            let Some(name) = entry.enclosed_name() else {
                continue;
            };
            let mut components = name.components();
            if wrapper.is_some() {
                components.next();
            }
            let relative: PathBuf = components.collect();
            if relative.as_os_str().is_empty() {
                continue;
            }
            let destination = target.join(&relative);
            if entry.is_dir() {
                fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
                continue;
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut file = fs::File::create(&destination)
                .map_err(|error| format!("{}: {error}", destination.display()))?;
            io::copy(&mut entry, &mut file)
                .map_err(|error| format!("{}: {error}", destination.display()))?;
        }
        if !target.join("localreel.exe").is_file() {
            return Err("the payload did not contain localreel.exe".to_string());
        }
        Ok(())
    }

    /// The single top-level directory every entry shares, if there is one.
    fn wrapper_folder(archive: &mut zip::ZipArchive<io::Cursor<&'static [u8]>>) -> Option<String> {
        let mut wrapper: Option<String> = None;
        for index in 0..archive.len() {
            let entry = archive.by_index(index).ok()?;
            let name = entry.enclosed_name()?;
            let mut components = name.components();
            let first = components.next()?.as_os_str().to_string_lossy().into_owned();
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
    fn finish_install(target: &Path) -> Result<(), String> {
        let self_copy = target.join("localreel-setup.exe");
        if let Ok(current) = std::env::current_exe() {
            if current != self_copy {
                fs::copy(&current, &self_copy)
                    .map_err(|error| format!("cannot copy the installer: {error}"))?;
            }
        }

        let estimated_kb = directory_size_kb(target);
        let manifest = format!(
            "{{\n  \"product\": \"LocalReel\",\n  \"version\": \"{VERSION}\",\n  \"build\": \"{BUILD_STAMP}\",\n  \"installed_at\": \"{}\",\n  \"install_dir\": {},\n  \"per_user\": true,\n  \"estimated_kb\": {estimated_kb}\n}}\n",
            timestamp(),
            json_string(&target.to_string_lossy()),
        );
        fs::write(target.join("install.json"), manifest)
            .map_err(|error| format!("cannot write install.json: {error}"))?;

        let launcher = if target.join("localreel-desktop.exe").is_file() {
            target.join("localreel-desktop.exe")
        } else {
            target.join("localreel.exe")
        };
        for shortcut in win::shortcut_paths() {
            if let Err(error) = win::create_shortcut(&shortcut, &launcher, target, "LocalReel") {
                // A missing Desktop is not worth failing the install for.
                eprintln!("shortcut {}: {error}", shortcut.display());
            }
        }

        let uninstall_string = format!("\"{}\" /uninstall", self_copy.display());
        win::register_uninstall(target, VERSION, estimated_kb, &uninstall_string)
            .map_err(|error| format!("cannot register the uninstall entry: {error}"))?;
        Ok(())
    }

    fn installed_version() -> Option<String> {
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        let key = hkcu.open_subkey_with_flags(win::UNINSTALL_KEY, winreg::enums::KEY_READ).ok()?;
        key.get_value::<String, _>("DisplayVersion").ok()
    }

    fn directory_size_kb(path: &Path) -> u64 {
        let mut total = 0u64;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                match entry.metadata() {
                    Ok(metadata) if metadata.is_dir() => total += directory_size_kb(&entry.path()),
                    Ok(metadata) => total += metadata.len(),
                    Err(_) => {}
                }
            }
        }
        total / 1024
    }

    fn timestamp() -> String {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string())
    }

    fn json_string(value: &str) -> String {
        let mut escaped = String::with_capacity(value.len() + 2);
        escaped.push('"');
        for character in value.chars() {
            match character {
                '"' => escaped.push_str("\\\""),
                '\\' => escaped.push_str("\\\\"),
                control if control.is_control() => escaped.push(' '),
                other => escaped.push(other),
            }
        }
        escaped.push('"');
        escaped
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
            Options::parse(
                args.iter().map(|value| value.to_string()),
                Lang::En.text(),
            )
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
        fn json_strings_are_escaped() {
            assert_eq!(json_string("C:\\Apps"), "\"C:\\\\Apps\"");
            assert_eq!(json_string("a\"b"), "\"a\\\"b\"");
        }
    }
}
