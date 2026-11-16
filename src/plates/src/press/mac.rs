//! The macOS half of [press](super): finds the button through the Accessibility API and performs
//! its press action.

use std::{ptr::NonNull, time::Instant};

use objc2_app_kit::NSRunningApplication;
use objc2_application_services::{
    AXCopyMultipleAttributeOptions, AXError, AXIsProcessTrustedWithOptions, AXUIElement,
    kAXTrustedCheckOptionPrompt,
};
use objc2_core_foundation::{CFArray, CFBoolean, CFDictionary, CFRetained, CFString, CFType, Type};
use objc2_foundation::NSString;

use super::{POLL, WAIT};

/// The Claude desktop app's bundle identifier.
const CLAUDE: &str = "com.anthropic.claudefordesktop";

pub(super) fn press(title: &str, choice: &str) -> Result<(), String> {
    // macOS lets a program read and press another app's controls only once you've turned the
    // program on under Privacy & Security > Accessibility, and with the prompt option macOS offers
    // to open that page the first time the program asks.
    // (documented, https://developer.apple.com/documentation/applicationservices/1459186-axisprocesstrustedwithoptions)
    let prompt = CFDictionary::from_slices(
        &[unsafe { kAXTrustedCheckOptionPrompt }],
        &[CFBoolean::new(true)],
    );
    if !unsafe { AXIsProcessTrustedWithOptions(Some(prompt.as_opaque())) } {
        return Err("Plates isn't turned on under Accessibility in System Settings".into());
    }
    let app = claude_app().ok_or("the Claude app isn't running")?;
    // The app is built on Electron, which turns on its accessibility features when a program sets
    // this attribute, and otherwise only while VoiceOver runs.
    // (documented, https://www.electronjs.org/docs/latest/tutorial/accessibility)
    unsafe {
        app.set_attribute_value(
            &CFString::from_static_str("AXManualAccessibility"),
            CFBoolean::new(true),
        )
    };
    let reader = Reader::new();
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(button) = reader.find_choice(&app, title, choice) {
            let err = unsafe { button.perform_action(&CFString::from_static_str("AXPress")) };
            return if err == AXError::Success {
                Ok(())
            } else {
                Err(format!("the press failed with AXError {}", err.0))
            };
        }
        if Instant::now() >= deadline {
            return Err("the session's button didn't show".into());
        }
        std::thread::sleep(POLL);
    }
}

fn claude_app() -> Option<CFRetained<AXUIElement>> {
    let apps =
        NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(CLAUDE));
    let pid = apps.firstObject()?.processIdentifier();
    Some(unsafe { AXUIElement::new_application(pid) })
}

/// What the search reads of one element, in one call to the app.
struct Node {
    role: String,
    title: String,
    description: String,
    children: Vec<CFRetained<AXUIElement>>,
}

impl Node {
    /// Whether the element's accessible name is `name`. Chromium puts a name that comes from an
    /// element's text in its title, and one from an `aria-label` in its description, so the
    /// search checks both.
    fn named(&self, name: &str) -> bool {
        self.title == name || self.description == name
    }
}

struct Reader {
    /// The attributes a `Node` holds, in its fields' order.
    attributes: CFRetained<CFArray<CFString>>,
}

impl Reader {
    fn new() -> Self {
        let names = ["AXRole", "AXTitle", "AXDescription", "AXChildren"].map(CFString::from_str);
        Self {
            attributes: CFArray::from_retained_objects(&names),
        }
    }

    // The button, in an app window showing the session titled `title`. The app's page for a
    // session is titled "<title> - Claude Code", so a button of the same label in another session
    // is pressed only if that session has the same title, and the wrap-up's buttons sit in a
    // group named "Suggested next actions", apart from the app's other buttons.
    fn find_choice(
        &self,
        app: &AXUIElement,
        title: &str,
        choice: &str,
    ) -> Option<CFRetained<AXUIElement>> {
        let document = format!("{title} - Claude Code");
        let page = self.first(app, |n| n.role == "AXWebArea" && n.named(&document))?;
        let group = self.first(&page, |n| n.named("Suggested next actions"))?;
        let button = self.first(&group, |n| n.role == "AXButton" && n.named(choice))?;
        enabled(&button).then_some(button)
    }

    // The first element under `root`, in the page's order, that `matches` accepts.
    fn first(
        &self,
        root: &AXUIElement,
        mut matches: impl FnMut(&Node) -> bool,
    ) -> Option<CFRetained<AXUIElement>> {
        let mut stack = vec![root.retain()];
        while let Some(element) = stack.pop() {
            let Some(node) = self.read(&element) else {
                continue;
            };
            if matches(&node) {
                return Some(element);
            }
            stack.extend(node.children.into_iter().rev());
        }
        None
    }

    fn read(&self, element: &AXUIElement) -> Option<Node> {
        let mut values: *const CFArray = std::ptr::null();
        let err = unsafe {
            element.copy_multiple_attribute_values(
                self.attributes.as_opaque(),
                AXCopyMultipleAttributeOptions::empty(),
                NonNull::from(&mut values),
            )
        };
        if err != AXError::Success {
            return None;
        }
        // An attribute the element lacks comes back as an error value in its place, which reads
        // as an empty string or no children.
        let values: CFRetained<CFArray<CFType>> =
            unsafe { CFRetained::from_raw(NonNull::new(values.cast_mut())?.cast()) };
        let text = |i| {
            values
                .get(i)
                .and_then(|v| v.downcast::<CFString>().ok())
                .map(|s| s.to_string())
                .unwrap_or_default()
        };
        let children = values
            .get(3)
            .and_then(|v| v.downcast::<CFArray>().ok())
            .map(|a| unsafe { CFRetained::cast_unchecked::<CFArray<AXUIElement>>(a) }.to_vec())
            .unwrap_or_default();
        Some(Node {
            role: text(0),
            title: text(1),
            description: text(2),
            children,
        })
    }
}

fn enabled(element: &AXUIElement) -> bool {
    let mut value: *const CFType = std::ptr::null();
    let err = unsafe {
        element.copy_attribute_value(
            &CFString::from_static_str("AXEnabled"),
            NonNull::from(&mut value),
        )
    };
    if err != AXError::Success {
        return false;
    }
    let Some(value) = NonNull::new(value.cast_mut()) else {
        return false;
    };
    let value = unsafe { CFRetained::from_raw(value) };
    value.downcast::<CFBoolean>().is_ok_and(|b| b.as_bool())
}
