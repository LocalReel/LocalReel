//! The windowed front end of the per-user installer.
//!
//! Double-clicking `localreel-setup.exe` should look like an installer: one
//! window that says what will happen, lets the user pick a folder, shows
//! progress, and offers to start LocalReel when it finishes.
//!
//! Everything that touches the disk lives in the parent module
//! (`perform_install`), so the window and the console flow cannot drift apart.
//! The console path stays for `/S` and scripted runs; if the window cannot be
//! created, `run` returns `None` and the caller falls back to it.

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, GetStockObject, GetSysColorBrush, SetBkMode, UpdateWindow, CLEARTYPE_QUALITY,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_GUI_FONT, DEFAULT_PITCH, FF_DONTCARE, HDC, HFONT,
    OUT_DEFAULT_PRECIS, TRANSPARENT,
};
use windows_sys::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_PROGRESS_CLASS, INITCOMMONCONTROLSEX,
};
use windows_sys::Win32::UI::HiDpi::{
    GetDpiForSystem, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, VK_ESCAPE};
use windows_sys::Win32::UI::Shell::{
    SHBrowseForFolderW, SHGetPathFromIDListW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateIconFromResourceEx, CreateWindowExW, DefWindowProcW, DestroyWindow,
    DispatchMessageW, GetMessageW, GetWindowLongPtrW, GetWindowTextW, IsDialogMessageW,
    LoadCursorW, MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassW, SendMessageW,
    SetWindowLongPtrW, SetWindowTextW, ShowWindow, TranslateMessage, MSG, WNDCLASSW,
};

use super::{perform_install, win, InstallFailure, Options, Text, VERSION};

// Window messages and styles the standard library does not re-export for us.
const WM_KEYDOWN: u32 = 0x0100;
const WM_CLOSE: u32 = 0x0010;
const WM_COMMAND: u32 = 0x0111;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const WM_DESTROY: u32 = 0x0002;
const WM_SETFONT: u32 = 0x0030;
const WM_SETICON: u32 = 0x0080;
const WM_APP: u32 = 0x8000;
const WM_APP_PROGRESS: u32 = WM_APP + 1;
const WM_APP_DONE: u32 = WM_APP + 2;

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_BORDER: u32 = 0x0080_0000;
const ES_AUTOHSCROLL: u32 = 0x0080;
const BS_DEFPUSHBUTTON: u32 = 0x0000_0001;
const BS_AUTOCHECKBOX: u32 = 0x0000_0003;
const SS_LEFT: u32 = 0x0000_0000;
const SS_NOPREFIX: u32 = 0x0080;
const SS_ETCHEDHORZ: u32 = 0x0010;
const BM_SETCHECK: u32 = 0x00F1;
const BM_GETCHECK: u32 = 0x00F0;
const BST_CHECKED: isize = 1;
const PBM_SETRANGE32: u32 = 0x0406;
const PBM_SETPOS: u32 = 0x0402;
const SW_SHOW: i32 = 5;
const SW_HIDE: i32 = 0;
const CW_USEDEFAULT: i32 = 0x8000_0000u32 as i32;
const GWLP_USERDATA: i32 = -21;
const GWLP_ID: i32 = -12;
const COLOR_WINDOW: i32 = 5;
const ICON_BIG: usize = 1;
const ICON_SMALL: usize = 0;
const IDC_ARROW: *const u16 = 32512 as *const u16;
const MB_OK: u32 = 0x0000_0000;
const MB_ICONERROR: u32 = 0x0000_0010;
const MB_ICONINFORMATION: u32 = 0x0000_0040;
const MB_ICONQUESTION: u32 = 0x0000_0020;
const MB_YESNO: u32 = 0x0000_0004;
const IDYES: i32 = 6;

const ID_INSTALL: usize = 1; // IDOK: Enter activates the default button
const ID_CANCEL: usize = 2; // IDCANCEL: Esc closes the window
const ID_BROWSE: usize = 101;
const ID_LAUNCH: usize = 102;

/// The icon ships next to the WiX source; embedding the bytes avoids a resource
/// compiler step just for a window icon.
const ICON_BYTES: &[u8] = include_bytes!("../../localreel.ico");

/// Everything the window procedure needs, reached through `GWLP_USERDATA`.
struct Ui {
    location_edit: HWND,
    status: HWND,
    progress: HWND,
    launch_check: HWND,
    install_button: HWND,
    browse_button: HWND,
    cancel_button: HWND,
    text: &'static Text,
    target: PathBuf,
    busy: bool,
    exit_code: u32,
    result: Arc<Mutex<Option<Result<(), InstallFailure>>>>,
    large_font: HFONT,
    body_font: HFONT,
    scale: f64,
}

/// Windows can always host a window; if creating it still fails, `run` reports
/// `None` and the console flow takes over.
pub fn available() -> bool {
    true
}

/// `None` means "no window"; the caller falls back to the console flow.
pub fn run(options: &Options, text: &'static Text) -> Option<u32> {
    unsafe {
        // Crisp text on scaled displays: the layout below is computed from the
        // system DPI rather than assuming 96.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // A GUI process started from Explorer still owns the console the
        // subsystem allocated; releasing it removes that stray black window.
        windows_sys::Win32::System::Console::FreeConsole();
        let _ = CoInitializeEx(
            std::ptr::null(),
            (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
        );
    }

    if options.uninstall {
        return Some(uninstall(options, text));
    }

    let dpi = unsafe { GetDpiForSystem() } as f64;
    let scale = if dpi <= 0.0 { 1.0 } else { dpi / 96.0 };

    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class_name = wide("LocalReelSetupWindow");
    let icon = unsafe { load_icon() };
    let mut window_class: WNDCLASSW = unsafe { std::mem::zeroed() };
    window_class.lpfnWndProc = Some(window_proc);
    window_class.hInstance = instance;
    window_class.lpszClassName = class_name.as_ptr();
    window_class.hCursor = unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) };
    window_class.hbrBackground = (COLOR_WINDOW + 1) as *mut c_void;
    window_class.hIcon = icon;
    if unsafe { RegisterClassW(&window_class) } == 0 {
        note(&format!(
            "RegisterClassW failed: {}",
            std::io::Error::last_os_error()
        ));
        return None;
    }

    unsafe {
        let mut controls: INITCOMMONCONTROLSEX = std::mem::zeroed();
        controls.dwSize = std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32;
        controls.dwICC = ICC_PROGRESS_CLASS;
        InitCommonControlsEx(&controls);
    }

    let title = wide(&text.format(text.app_title, &[("version", VERSION)]));
    let window_style = WS_OVERLAPPEDWINDOW & !0x0002_0000 & !0x0001_0000; // no maximize / no resize
    let client = (scaled(600, scale), scaled(344, scale));
    let mut rect = windows_sys::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: client.0,
        bottom: client.1,
    };
    unsafe { AdjustWindowRectEx(&mut rect, window_style, 0, 0) };

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            window_style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        note(&format!(
            "CreateWindowExW failed: {}",
            std::io::Error::last_os_error()
        ));
        return None;
    }

    let body_font = unsafe { GetStockObject(DEFAULT_GUI_FONT) as HFONT };
    let large_font = unsafe {
        CreateFontW(
            -scaled(22, scale),
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            wide("Segoe UI").as_ptr(),
        )
    };

    // A previous run's folder beats the compiled default, unless /D said
    // otherwise.
    let initial = if options.explicit_dir {
        options.install_dir.clone()
    } else {
        win::last_install_dir().unwrap_or_else(|| options.install_dir.clone())
    };

    let mut state = Box::new(Ui {
        location_edit: std::ptr::null_mut(),
        status: std::ptr::null_mut(),
        progress: std::ptr::null_mut(),
        launch_check: std::ptr::null_mut(),
        install_button: std::ptr::null_mut(),
        browse_button: std::ptr::null_mut(),
        cancel_button: std::ptr::null_mut(),
        text,
        target: initial,
        busy: false,
        exit_code: 2,
        result: Arc::new(Mutex::new(None)),
        large_font,
        body_font,
        scale,
    });

    unsafe {
        create_children(hwnd, instance, state.as_mut());
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        if !icon.is_null() {
            SendMessageW(hwnd, WM_SETICON, ICON_BIG, icon as isize);
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL, icon as isize);
        }
    }

    let mut message: MSG = unsafe { std::mem::zeroed() };
    loop {
        let result = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }
        let handled = unsafe { IsDialogMessageW(hwnd, &message) };
        if handled == 0 {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    unsafe {
        let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui;
        if !raw.is_null() {
            let state = Box::from_raw(raw);
            let code = state.exit_code;
            delete_font(state.large_font);
            return Some(code);
        }
    }
    Some(2)
}

/// Uninstall confirmation and result, without a window: the action is a single
/// yes/no question, and Apps & Features hides whatever console we would use.
fn uninstall(options: &Options, text: &'static Text) -> u32 {
    let target = &options.install_dir;
    let caption = wide(text.uninstall_title);
    let question = wide(text.uninstall_confirm);
    let answer = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            question.as_ptr(),
            caption.as_ptr(),
            MB_YESNO | MB_ICONQUESTION,
        )
    };
    if answer != IDYES {
        return 2;
    }

    unsafe {
        let detached = win::detach_running_copy(target);
        for shortcut in win::shortcut_paths() {
            let _ = std::fs::remove_file(shortcut);
        }
        let _ = win::unregister_uninstall();
        super::remove_program_files(target);
        if let Some(temp) = detached {
            let _ = std::fs::remove_file(temp);
        }
        if target.join("localreel.exe").is_file() {
            let message =
                wide(&text.format(text.failed, &[("error", "localreel.exe is still in place")]));
            MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                caption.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
            return 1;
        }

        let libraries = [win::data_dir(), target.join("data")];
        let mut lines: Vec<String> = Vec::new();
        if options.remove_data {
            for data in libraries {
                if !data.is_dir() {
                    continue;
                }
                match win::remove_dir_all_with_retry(&data) {
                    Ok(()) => lines
                        .push(text.format(text.data_removed, &[("dir", &data.to_string_lossy())])),
                    Err(error) => {
                        lines.push(text.format(text.failed, &[("error", &error.to_string())]))
                    }
                }
            }
        } else {
            for data in libraries {
                if data.is_dir() {
                    lines.push(text.format(text.data_kept, &[("dir", &data.to_string_lossy())]));
                }
            }
        }
        lines.insert(0, text.uninstall_done.to_string());
        let message = wide(&lines.join("\r\n\r\n"));
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
    0
}

/// Fill the window with its controls. Every coordinate below is for 96 DPI and
/// goes through `scaled`.
unsafe fn create_children(hwnd: HWND, instance: *mut c_void, state: &mut Ui) {
    let scale = state.scale;
    let heading = wide(
        &state
            .text
            .format(state.text.gui_heading, &[("version", VERSION)]),
    );
    let mut body = format!("{}\r\n{}\r\n\r\n", state.text.intro, state.text.per_user);
    if let Some(installed) = super::installed_version() {
        body.push_str(
            &state
                .text
                .format(state.text.already_installed, &[("version", &installed)]),
        );
    }
    let body = wide(&body);
    let location_label = wide(state.text.gui_location);
    let data_line = wide(&state.text.format(
        state.text.data_dir,
        &[("dir", &win::data_dir().to_string_lossy())],
    ));
    let install_label = wide(state.text.gui_install);
    let browse_label = wide(state.text.gui_browse);
    let cancel_label = wide(state.text.gui_cancel);
    let launch_label = wide(state.text.gui_launch_after);
    let target = wide(&state.target.to_string_lossy());

    let heading_box = child(
        instance,
        hwnd,
        "STATIC",
        &heading,
        SS_LEFT | SS_NOPREFIX,
        scaled(24, scale),
        scaled(18, scale),
        scaled(552, scale),
        scaled(30, scale),
    );
    font(heading_box, state.large_font as usize);

    let body_box = child(
        instance,
        hwnd,
        "STATIC",
        &body,
        SS_LEFT | SS_NOPREFIX,
        scaled(24, scale),
        scaled(56, scale),
        scaled(552, scale),
        scaled(74, scale),
    );
    font(body_box, state.body_font as usize);

    let label = child(
        instance,
        hwnd,
        "STATIC",
        &location_label,
        SS_LEFT | SS_NOPREFIX,
        scaled(24, scale),
        scaled(146, scale),
        scaled(552, scale),
        scaled(18, scale),
    );
    font(label, state.body_font as usize);

    let edit = child(
        instance,
        hwnd,
        "EDIT",
        &target,
        WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL,
        scaled(24, scale),
        scaled(168, scale),
        scaled(400, scale),
        scaled(26, scale),
    );
    font(edit, state.body_font as usize);
    state.location_edit = edit;

    let browse = child(
        instance,
        hwnd,
        "BUTTON",
        &browse_label,
        WS_TABSTOP,
        scaled(432, scale),
        scaled(167, scale),
        scaled(120, scale),
        scaled(28, scale),
    );
    font(browse, state.body_font as usize);
    identify(browse, ID_BROWSE);
    state.browse_button = browse;

    let library = child(
        instance,
        hwnd,
        "STATIC",
        &data_line,
        SS_LEFT | SS_NOPREFIX,
        scaled(24, scale),
        scaled(204, scale),
        scaled(552, scale),
        scaled(18, scale),
    );
    font(library, state.body_font as usize);

    let progress = child(
        instance,
        hwnd,
        "msctls_progress32",
        &wide(""),
        0,
        scaled(24, scale),
        scaled(232, scale),
        scaled(552, scale),
        scaled(14, scale),
    );
    if !progress.is_null() {
        SendMessageW(progress, PBM_SETRANGE32, 0, 100);
        ShowWindow(progress, SW_HIDE);
    }
    state.progress = progress;

    let status = child(
        instance,
        hwnd,
        "STATIC",
        &wide(""),
        SS_LEFT | SS_NOPREFIX,
        scaled(24, scale),
        scaled(254, scale),
        scaled(552, scale),
        scaled(18, scale),
    );
    font(status, state.body_font as usize);
    state.status = status;

    let separator = child(
        instance,
        hwnd,
        "STATIC",
        &wide(""),
        SS_ETCHEDHORZ,
        scaled(24, scale),
        scaled(282, scale),
        scaled(552, scale),
        scaled(2, scale),
    );
    if !separator.is_null() {
        font(separator, state.body_font as usize);
    }

    let launch = child(
        instance,
        hwnd,
        "BUTTON",
        &launch_label,
        WS_TABSTOP | BS_AUTOCHECKBOX,
        scaled(24, scale),
        scaled(294, scale),
        scaled(320, scale),
        scaled(24, scale),
    );
    font(launch, state.body_font as usize);
    identify(launch, ID_LAUNCH);
    SendMessageW(launch, BM_SETCHECK, BST_CHECKED as usize, 0);
    state.launch_check = launch;

    let install = child(
        instance,
        hwnd,
        "BUTTON",
        &install_label,
        WS_TABSTOP | BS_DEFPUSHBUTTON,
        scaled(360, scale),
        scaled(292, scale),
        scaled(100, scale),
        scaled(32, scale),
    );
    font(install, state.body_font as usize);
    identify(install, ID_INSTALL);
    state.install_button = install;

    let cancel = child(
        instance,
        hwnd,
        "BUTTON",
        &cancel_label,
        WS_TABSTOP,
        scaled(472, scale),
        scaled(292, scale),
        scaled(104, scale),
        scaled(32, scale),
    );
    font(cancel, state.body_font as usize);
    identify(cancel, ID_CANCEL);
    state.cancel_button = cancel;
}

// The Win32 call itself takes this many positional values; bundling them into a
// struct would hide which one is width and which one is x for the reader.
#[allow(clippy::too_many_arguments)]
unsafe fn child(
    instance: *mut c_void,
    parent: HWND,
    class: &str,
    text: &[u16],
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> HWND {
    let class = wide(class);
    CreateWindowExW(
        0,
        class.as_ptr(),
        text.as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        x,
        y,
        width,
        height,
        parent,
        std::ptr::null_mut(),
        instance,
        std::ptr::null(),
    )
}

/// Windows sends the control's ID with every `WM_COMMAND`. A child created with
/// a null menu handle starts out with ID 0, so every button would look alike;
/// this stamps the real one.
unsafe fn identify(control: HWND, id: usize) {
    SetWindowLongPtrW(control, GWLP_ID, id as isize);
}

unsafe fn font(control: HWND, font: usize) {
    SendMessageW(control, WM_SETFONT, font, 1);
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_COMMAND => {
            let id = wparam & 0xffff;
            match id {
                ID_INSTALL => start(hwnd),
                ID_CANCEL => cancel(hwnd),
                ID_BROWSE => choose_directory(hwnd),
                _ => {}
            }
            0
        }
        WM_KEYDOWN if wparam == VK_ESCAPE as usize => {
            cancel(hwnd);
            0
        }
        WM_CLOSE => {
            cancel(hwnd);
            0
        }
        WM_CTLCOLORSTATIC => {
            // Static controls otherwise paint the button-face grey, which shows
            // up as boxes on the window's white background.
            let device = wparam as HDC;
            SetBkMode(device, TRANSPARENT as i32);
            GetSysColorBrush(COLOR_WINDOW) as LRESULT
        }
        WM_APP_PROGRESS => {
            if let Some(state) = state(hwnd) {
                let percent = wparam.min(100);
                if !state.progress.is_null() {
                    SendMessageW(state.progress, PBM_SETPOS, percent, 0);
                }
                let percent_text = percent.to_string();
                let line = state
                    .text
                    .format(state.text.gui_installing, &[("percent", &percent_text)]);
                let line = wide(&line);
                SetWindowTextW(state.status, line.as_ptr());
            }
            0
        }
        WM_APP_DONE => {
            finish(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn state(hwnd: HWND) -> Option<&'static mut Ui> {
    let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui;
    raw.as_mut()
}

unsafe fn start(hwnd: HWND) {
    let Some(state) = state(hwnd) else { return };
    if state.busy {
        return;
    }
    let mut buffer = vec![0u16; 1024];
    let length = GetWindowTextW(
        state.location_edit,
        buffer.as_mut_ptr(),
        buffer.len() as i32,
    );
    let target = if length > 0 {
        PathBuf::from(String::from_utf16_lossy(&buffer[..length as usize]))
    } else {
        state.target.clone()
    };
    if target.as_os_str().is_empty() {
        return;
    }

    state.busy = true;
    state.target = target.clone();
    EnableWindow(state.location_edit, 0);
    EnableWindow(state.browse_button, 0);
    EnableWindow(state.install_button, 0);
    EnableWindow(state.cancel_button, 0);
    EnableWindow(state.launch_check, 0);
    if !state.progress.is_null() {
        SendMessageW(state.progress, PBM_SETPOS, 0, 0);
        ShowWindow(state.progress, SW_SHOW);
    }

    let slot = Arc::clone(&state.result);
    let hwnd_value = hwnd as isize;
    std::thread::spawn(move || {
        let mut progress = |done: usize, total: usize| {
            let percent = (done * 100).checked_div(total).unwrap_or(0);
            unsafe {
                PostMessageW(
                    hwnd_value as *mut c_void,
                    WM_APP_PROGRESS,
                    percent.min(100),
                    0,
                );
            }
        };
        let outcome = perform_install(&target, false, &mut progress);
        if let Ok(mut guard) = slot.lock() {
            *guard = Some(outcome);
        }
        unsafe {
            PostMessageW(hwnd_value as *mut c_void, WM_APP_DONE, 0, 0);
        }
    });
}

unsafe fn finish(hwnd: HWND) {
    let Some(state) = state(hwnd) else { return };
    let outcome = state.result.lock().ok().and_then(|mut guard| guard.take());
    let Some(outcome) = outcome else { return };

    match outcome {
        Ok(()) => {
            state.exit_code = 0;
            win::remember_install_dir(&state.target);
            let body = wide(&state.text.format(state.text.done, &[("version", VERSION)]));
            let caption = wide(state.text.gui_done_title);
            MessageBoxW(
                hwnd,
                body.as_ptr(),
                caption.as_ptr(),
                MB_OK | MB_ICONINFORMATION,
            );
            let launch = SendMessageW(state.launch_check, BM_GETCHECK, 0, 0) == BST_CHECKED;
            if launch {
                super::launch_installed(&state.target);
            }
            DestroyWindow(hwnd);
        }
        Err(failure) => {
            state.busy = false;
            EnableWindow(state.location_edit, 1);
            EnableWindow(state.browse_button, 1);
            EnableWindow(state.install_button, 1);
            EnableWindow(state.cancel_button, 1);
            EnableWindow(state.launch_check, 1);
            if !state.progress.is_null() {
                ShowWindow(state.progress, SW_HIDE);
            }
            let message = match failure {
                InstallFailure::Busy => state.text.busy.to_string(),
                InstallFailure::Failed(error) => {
                    state.text.format(state.text.failed, &[("error", &error)])
                }
            };
            let message = wide(&message);
            let caption = wide(state.text.gui_error_title);
            MessageBoxW(
                hwnd,
                message.as_ptr(),
                caption.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}

unsafe fn cancel(hwnd: HWND) {
    let Some(state) = state(hwnd) else { return };
    if state.busy {
        return;
    }
    state.exit_code = 2;
    DestroyWindow(hwnd);
}

unsafe fn choose_directory(hwnd: HWND) {
    let Some(state) = state(hwnd) else { return };
    if state.busy {
        return;
    }
    let title = wide(state.text.gui_location);
    let mut display = vec![0u16; 260];
    let mut info: BROWSEINFOW = std::mem::zeroed();
    info.hwndOwner = hwnd;
    info.pszDisplayName = display.as_mut_ptr();
    info.lpszTitle = title.as_ptr();
    info.ulFlags = BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE;
    let item = SHBrowseForFolderW(&info);
    if item.is_null() {
        return;
    }
    let mut buffer = vec![0u16; 1024];
    let ok = SHGetPathFromIDListW(item, buffer.as_mut_ptr());
    CoTaskMemFree(item as *const c_void);
    if ok == 0 {
        return;
    }
    let length = buffer.iter().position(|&unit| unit == 0).unwrap_or(0);
    let chosen = wide(&String::from_utf16_lossy(&buffer[..length]));
    SetWindowTextW(state.location_edit, chosen.as_ptr());
}

/// Pull the largest image out of the embedded `.ico` and turn it into an HICON.
unsafe fn load_icon() -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    let bytes = ICON_BYTES;
    if bytes.len() < 22 {
        return std::ptr::null_mut();
    }
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    let mut best: Option<(usize, usize)> = None;
    for index in 0..count {
        let entry = 6 + index * 16;
        if entry + 16 > bytes.len() {
            break;
        }
        let size = u32::from_le_bytes([
            bytes[entry + 8],
            bytes[entry + 9],
            bytes[entry + 10],
            bytes[entry + 11],
        ]) as usize;
        let offset = u32::from_le_bytes([
            bytes[entry + 12],
            bytes[entry + 13],
            bytes[entry + 14],
            bytes[entry + 15],
        ]) as usize;
        if offset + size <= bytes.len() && best.is_none_or(|(_, current)| size > current) {
            best = Some((offset, size));
        }
    }
    let Some((offset, size)) = best else {
        return std::ptr::null_mut();
    };
    CreateIconFromResourceEx(
        bytes[offset..offset + size].as_ptr(),
        size as u32,
        1,
        0x0003_0000,
        0,
        0,
        0,
    )
}

unsafe fn delete_font(font: HFONT) {
    if !font.is_null() {
        windows_sys::Win32::Graphics::Gdi::DeleteObject(font);
    }
}

fn scaled(value: i32, scale: f64) -> i32 {
    (value as f64 * scale).round() as i32
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Leave a short breadcrumb when the window cannot be created: the fallback is
/// a console the user may never see, so silence would be a support dead end.
fn note(message: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("localreel-setup-gui.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{message}");
    }
}
