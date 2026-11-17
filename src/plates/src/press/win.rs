//! The Windows half of [press](super): finds the button through UI Automation and invokes it.

use std::{mem::ManuallyDrop, time::Instant};

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM},
        System::{
            Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
            Variant::{VARIANT, VT_BSTR, VT_I4, VariantClear},
        },
        UI::{
            Accessibility::{
                CUIAutomation8, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
                TreeScope_Descendants, UIA_ButtonControlTypeId, UIA_ControlTypePropertyId,
                UIA_DocumentControlTypeId, UIA_GroupControlTypeId, UIA_InvokePatternId,
                UIA_NamePropertyId,
            },
            WindowsAndMessaging::{EnumWindows, IsWindowVisible},
        },
    },
    core::{BOOL, BSTR},
};

use super::{POLL, WAIT};

pub(super) fn press(title: &str, choice: &str) -> windows::core::Result<()> {
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

// The app's visible top-level windows, as `native::claude_app_window` tells them.
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
        .filter(|&hwnd| crate::native::claude_app_window(hwnd))
        .collect()
}
