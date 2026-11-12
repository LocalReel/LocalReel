//! Installer text in the language Windows is already using.
//!
//! One binary, two languages today. The dictionary is a struct rather than a
//! lookup table so a missing string is a compile error instead of a blank line
//! in front of a user; adding a language means adding one struct and one arm in
//! [`Lang::detect`].

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    /// The language the current user's Windows UI is in.
    pub fn detect() -> Self {
        if let Ok(value) = std::env::var("LOCALREEL_LANG") {
            if let Some(lang) = Self::parse(&value) {
                return lang;
            }
        }
        #[cfg(windows)]
        {
            // GetUserDefaultUILanguage returns a LANGID; the low 10 bits are the
            // primary language, and 0x04 is Chinese in every script.
            let lcid = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
            if lcid & 0x3ff == 0x04 {
                return Self::Zh;
            }
        }
        Self::En
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" | "zh-hans" | "chinese" => Some(Self::Zh),
            "en" | "en-us" | "english" => Some(Self::En),
            _ => None,
        }
    }

    pub fn text(self) -> &'static Text {
        match self {
            Self::Zh => &ZH,
            Self::En => &EN,
        }
    }
}

pub struct Text {
    pub app_title: &'static str,
    pub intro: &'static str,
    pub install_dir: &'static str,
    pub data_dir: &'static str,
    pub per_user: &'static str,
    pub confirm: &'static str,
    pub cancelled: &'static str,
    pub installing: &'static str,
    pub done: &'static str,
    pub launch_question: &'static str,
    pub busy: &'static str,
    pub uninstall_title: &'static str,
    pub uninstall_confirm: &'static str,
    pub uninstalling: &'static str,
    pub uninstall_done: &'static str,
    pub data_kept: &'static str,
    pub data_removed: &'static str,
    pub already_installed: &'static str,
    pub failed: &'static str,
    pub press_enter: &'static str,
    pub unknown_argument: &'static str,
    pub help: &'static str,
    // Graphical installer window.
    pub gui_location: &'static str,
    pub gui_browse: &'static str,
    pub gui_install: &'static str,
    pub gui_cancel: &'static str,
    pub gui_installing: &'static str,
    pub gui_done_title: &'static str,
    pub gui_error_title: &'static str,
}

static ZH: Text = Text {
    app_title: "LocalReel {version} 安装程序",
    intro: "将把 LocalReel 安装到当前用户目录，不需要管理员权限，也不会影响这台电脑上的其他账户。",
    install_dir: "安装位置：{dir}",
    data_dir: "媒体库位置：{dir}（卸载时保留）",
    per_user: "只安装给当前用户；想装给所有用户请使用 MSI 安装包。",
    confirm: "按回车开始安装，输入 n 取消 [Y/n]：",
    cancelled: "已取消，未做任何修改。",
    installing: "正在安装…",
    done: "安装完成：{version}",
    launch_question: "现在启动 LocalReel？[Y/n] ",
    busy: "LocalReel 正在运行，请先退出再安装。",
    uninstall_title: "LocalReel 卸载程序",
    uninstall_confirm: "确定卸载 LocalReel？媒体库会保留。 [y/N] ",
    uninstalling: "正在卸载…",
    uninstall_done: "已卸载 LocalReel。",
    data_kept: "媒体库保留在：{dir}",
    data_removed: "媒体库已删除：{dir}",
    already_installed: "已检测到 LocalReel {version}，将覆盖安装。",
    failed: "安装失败：{error}",
    press_enter: "按回车退出…",
    unknown_argument: "无法识别的参数：{argument}（使用 /? 查看用法）",
    help: "用法：localreel-setup.exe [/S] [/uninstall] [/removedata] [/lang=zh|en] [/D=<目录>] [/wait] [/launch]",
    gui_location: "安装位置：",
    gui_browse: "浏览…",
    gui_install: "安装",
    gui_cancel: "取消",
    gui_installing: "正在安装… {percent}%",
    gui_done_title: "安装完成",
    gui_error_title: "安装失败",
};

static EN: Text = Text {
    app_title: "LocalReel {version} Setup",
    intro: "Installs LocalReel for the current user. No administrator rights are needed and no other account on this PC is touched.",
    install_dir: "Install location: {dir}",
    data_dir: "Library location: {dir} (kept when uninstalling)",
    per_user: "Installs for this user only; use the MSI package to install for everyone.",
    confirm: "Press Enter to install, type n to cancel [Y/n]: ",
    cancelled: "Cancelled, nothing was changed.",
    installing: "Installing…",
    done: "Installed {version}.",
    launch_question: "Start LocalReel now? [Y/n] ",
    busy: "LocalReel is running. Close it and run Setup again.",
    uninstall_title: "LocalReel Uninstall",
    uninstall_confirm: "Remove LocalReel? Your library is kept. [y/N] ",
    uninstalling: "Uninstalling…",
    uninstall_done: "LocalReel was removed.",
    data_kept: "Library kept in: {dir}",
    data_removed: "Library removed: {dir}",
    already_installed: "LocalReel {version} is already installed; it will be replaced.",
    failed: "Setup failed: {error}",
    press_enter: "Press Enter to close…",
    unknown_argument: "Unknown argument: {argument} (use /? for usage)",
    help: "Usage: localreel-setup.exe [/S] [/uninstall] [/removedata] [/lang=zh|en] [/D=<dir>] [/wait] [/launch]",
    gui_location: "Install location:",
    gui_browse: "Browse…",
    gui_install: "Install",
    gui_cancel: "Cancel",
    gui_installing: "Installing… {percent}%",
    gui_done_title: "Installation complete",
    gui_error_title: "Setup failed",
};

impl Text {
    pub fn format(&self, template: &str, pairs: &[(&str, &str)]) -> String {
        let mut text = template.to_string();
        for (name, value) in pairs {
            text = text.replace(&format!("{{{name}}}"), value);
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_is_complete_and_distinct() {
        // A missing field cannot happen (the struct would not compile), but a
        // copy-paste that leaves English text in the Chinese dictionary can.
        assert_ne!(ZH.app_title, EN.app_title);
        assert_ne!(ZH.done, EN.done);
        assert_ne!(ZH.uninstall_done, EN.uninstall_done);
    }

    #[test]
    fn formats_placeholders() {
        let text = Lang::En
            .text()
            .format("Installed {version}.", &[("version", "0.5.0")]);
        assert_eq!(text, "Installed 0.5.0.");
    }

    #[test]
    fn parses_language_names() {
        assert_eq!(Lang::parse("zh-CN"), Some(Lang::Zh));
        assert_eq!(Lang::parse("EN"), Some(Lang::En));
        assert_eq!(Lang::parse("de"), None);
    }
}
