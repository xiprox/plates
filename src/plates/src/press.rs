//! Answering a session's wrap-up from its hover card, by pressing the Claude desktop app's own
//! button for the choice through UI Automation, as a click on it would.
//!
//! No supported channel lets a program outside the app send a message into one of its sessions,
//! and a press through UI Automation reaches the session's transcript as the same `user` record a
//! click writes (measured on Windows 11 25H2, 2026-11-15, record/areas/hooks/log.md).

use std::{
    mem::ManuallyDrop,
    time::{Duration, Instant},
};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HWND, LPARAM},
        System::{
            Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
            Variant::{VARIANT, VT_BSTR, VT_I4, VariantClear},
        },
        UI::{
            Accessibility::{
                CUIAutomation8, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
                TreeScope_Descendants, UIA_ButtonControlTypeId, UIA_ControlTypePropertyId,
                UIA_DocumentControlTypeId, UIA_GroupControlTypeId, UIA_InvokePatternId,
                UIA_NamePropertyId,
            },
            WindowsAndMessaging::{
                EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible,
            },
        },
    },
    core::{BOOL, BSTR, PWSTR},
};

/// How long the press waits for the app to show the session and its button. The app showed a
/// session's wrap-up within 843ms of its link, so this leaves room for a slower switch.
const WAIT: Duration = Duration::from_secs(3);

/// How often the press looks again while it waits.
const POLL: Duration = Duration::from_millis(100);

/// Presses the button labeled `choice` under the wrap-up of the session titled `title`, once the
/// app shows that session, on a thread of its own. If the button doesn't show within `WAIT`, it
/// presses nothing, and the session stays open in the app for you to answer.
pub fn press_choice(title: String, choice: String) {
    let spawned = std::thread::Builder::new()
        .name("plates-press".into())
        .spawn(move || {
            if let Err(e) = press(&title, &choice) {
                eprintln!("Couldn't press \u{201c}{choice}\u{201d} in the Claude app: {e}");
            }
        });
    if let Err(e) = spawned {
        eprintln!("Couldn't start a thread to press a choice: {e}");
    }
}

fn press(title: &str, choice: &str) -> windows::core::Result<()> {
    // UI Automation calls the app's process and waits for it, which mustn't happen on the rack's
    // thread, and Microsoft recommends a client use the multithreaded apartment.
    // (documented, https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading)
    let uia: IUIAutomation = unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?
    };
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(button) = find_choice(&uia, title, choice) {
            let invoke: IUIAutomationInvokePattern =
                unsafe { button.GetCurrentPatternAs(UIA_InvokePatternId)? };
            return unsafe { invoke.Invoke() };
        }
        if Instant::now() >= deadline {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x8000_4005u32 as i32),
                "the session's button didn't show",
            ));
        }
        std::thread::sleep(POLL);
    }
}

// The button, in an app window showing the session titled `title`. The app's web document is
// named "<title> - Claude Code", so a button of the same label in another session is pressed only
// if that session has the same title, and the wrap-up's buttons sit in a group named "Suggested
// next actions", apart from the app's other buttons (measured on Windows 11 25H2, 2026-11-15,
// record/areas/hooks/log.md).
fn find_choice(uia: &IUIAutomation, title: &str, choice: &str) -> Option<IUIAutomationElement> {
    let document = format!("{title} - Claude Code");
    app_windows().into_iter().find_map(|hwnd| unsafe {
        let root = uia.ElementFromHandle(hwnd).ok()?;
        let page = first(uia, &root, &document, UIA_DocumentControlTypeId.0)?;
        let group = first(
            uia,
            &page,
            "Suggested next actions",
            UIA_GroupControlTypeId.0,
        )?;
        let button = first(uia, &group, choice, UIA_ButtonControlTypeId.0)?;
        button
            .CurrentIsEnabled()
            .is_ok_and(|enabled| enabled.as_bool())
            .then_some(button)
    })
}

// The first element under `root` with this name and control type.
unsafe fn first(
    uia: &IUIAutomation,
    root: &IUIAutomationElement,
    name: &str,
    control_type: i32,
) -> Option<IUIAutomationElement> {
    let mut by_name = variant_str(name);
    let mut by_type = variant_i4(control_type);
    let found = unsafe {
        let condition = uia
            .CreatePropertyCondition(UIA_NamePropertyId, &by_name)
            .and_then(|n| {
                uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &by_type)
                    .and_then(|t| uia.CreateAndCondition(&n, &t))
            });
        let _ = VariantClear(&mut by_name);
        let _ = VariantClear(&mut by_type);
        root.FindFirst(TreeScope_Descendants, &condition.ok()?)
    };
    found.ok()
}

fn variant_str(s: &str) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        let inner = &mut *v.Anonymous.Anonymous;
        inner.vt = VT_BSTR;
        inner.Anonymous.bstrVal = ManuallyDrop::new(BSTR::from(s));
    }
    v
}

fn variant_i4(n: i32) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        let inner = &mut *v.Anonymous.Anonymous;
        inner.vt = VT_I4;
        inner.Anonymous.lVal = n;
    }
    v
}

// The app's visible top-level windows: Chromium's `Chrome_WidgetWin_1` windows of a process whose
// image is `claude.exe`.
fn app_windows() -> Vec<HWND> {
    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let found = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
        if unsafe { IsWindowVisible(hwnd) }.as_bool() {
            found.push(hwnd);
        }
        BOOL(1)
    }
    let mut visible: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut visible as *mut _ as isize));
    }
    visible
        .into_iter()
        .filter(|&hwnd| class_name(hwnd) == "Chrome_WidgetWin_1" && is_app(hwnd))
        .collect()
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn is_app(hwnd: HWND) -> bool {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    let Ok(process) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) })
    else {
        return false;
    };
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let read = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    };
    unsafe {
        let _ = CloseHandle(process);
    }
    read.is_ok()
        && String::from_utf16_lossy(&buf[..len as usize])
            .to_ascii_lowercase()
            .ends_with("\\claude.exe")
}
