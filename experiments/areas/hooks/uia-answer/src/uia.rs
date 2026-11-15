//! One walk of a window's UI Automation tree, keeping for each element the properties the probe
//! reports, and nothing it would have to invoke.

use std::mem::ManuallyDrop;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Variant::{VARIANT, VT_BOOL, VT_BSTR, VT_I4, VariantClear};
use windows::Win32::UI::Accessibility::*;
use windows::core::{BSTR, Result};

/// The properties every walk caches. The value's text is read only for its length, since a link's
/// value is its URL and the composer's is whatever you've typed.
const PROPS: &[UIA_PROPERTY_ID] = &[
    UIA_NamePropertyId,
    UIA_ControlTypePropertyId,
    UIA_AutomationIdPropertyId,
    UIA_ClassNamePropertyId,
    UIA_AriaRolePropertyId,
    UIA_FrameworkIdPropertyId,
    UIA_ProviderDescriptionPropertyId,
    UIA_BoundingRectanglePropertyId,
    UIA_IsOffscreenPropertyId,
    UIA_IsEnabledPropertyId,
    UIA_IsKeyboardFocusablePropertyId,
    UIA_HasKeyboardFocusPropertyId,
    UIA_IsInvokePatternAvailablePropertyId,
    UIA_IsValuePatternAvailablePropertyId,
    UIA_ValueIsReadOnlyPropertyId,
    UIA_ValueValuePropertyId,
    UIA_IsTextPatternAvailablePropertyId,
    UIA_LegacyIAccessibleDefaultActionPropertyId,
    UIA_IsSelectionItemPatternAvailablePropertyId,
    UIA_SelectionItemIsSelectedPropertyId,
];

pub struct Node {
    pub el: IUIAutomationElement,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub depth: usize,
    pub name: String,
    pub control_type: i32,
    pub automation_id: String,
    pub class_name: String,
    pub aria_role: String,
    pub framework_id: String,
    pub provider: String,
    pub rect: RECT,
    pub offscreen: bool,
    pub enabled: bool,
    pub focusable: bool,
    pub focused: bool,
    pub invoke: bool,
    pub value: bool,
    pub value_read_only: Option<bool>,
    pub value_len: Option<usize>,
    pub text: bool,
    pub legacy_action: String,
    pub selected: Option<bool>,
}

pub struct Tree {
    pub nodes: Vec<Node>,
    pub took: Duration,
    pub method: &'static str,
}

pub fn client() -> Result<IUIAutomation> {
    use windows::Win32::System::Com::*;
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
    }
}

/// Walks the raw view under `hwnd`, which includes every element a provider exposes. It asks for
/// the whole subtree in one cached request, and falls back to a walker, one element per call, if
/// the provider refuses that.
pub fn walk(uia: &IUIAutomation, hwnd: HWND, limit: usize) -> Result<Tree> {
    let start = Instant::now();
    unsafe {
        let root = uia.ElementFromHandle(hwnd)?;
        let request = uia.CreateCacheRequest()?;
        for &p in PROPS {
            request.AddProperty(p)?;
        }
        request.SetTreeFilter(&uia.RawViewCondition()?)?;
        request.SetTreeScope(TreeScope_Subtree)?;
        let mut nodes = Vec::new();
        let method = match root.BuildUpdatedCache(&request) {
            Ok(cached) => {
                collect_cached(cached, &mut nodes, limit);
                "one cached subtree request"
            }
            Err(_) => {
                request.SetTreeScope(TreeScope_Element)?;
                collect_walker(uia, &root, &request, &mut nodes, limit)?;
                "raw view walker"
            }
        };
        Ok(Tree {
            nodes,
            took: start.elapsed(),
            method,
        })
    }
}

unsafe fn collect_cached(root: IUIAutomationElement, nodes: &mut Vec<Node>, limit: usize) {
    let mut stack = vec![(root, None, 0)];
    while let Some((el, parent, depth)) = stack.pop() {
        if nodes.len() >= limit {
            break;
        }
        let i = push(nodes, el.clone(), parent, depth);
        if let Ok(kids) = unsafe { el.GetCachedChildren() } {
            let n = unsafe { kids.Length() }.unwrap_or(0);
            for k in (0..n).rev() {
                if let Ok(c) = unsafe { kids.GetElement(k) } {
                    stack.push((c, Some(i), depth + 1));
                }
            }
        }
    }
}

unsafe fn collect_walker(
    uia: &IUIAutomation,
    root: &IUIAutomationElement,
    request: &IUIAutomationCacheRequest,
    nodes: &mut Vec<Node>,
    limit: usize,
) -> Result<()> {
    let walker = unsafe { uia.RawViewWalker()? };
    let cached_root = unsafe { root.BuildUpdatedCache(request)? };
    let mut stack = vec![(cached_root, None, 0)];
    while let Some((el, parent, depth)) = stack.pop() {
        if nodes.len() >= limit {
            break;
        }
        let i = push(nodes, el.clone(), parent, depth);
        let mut kids = Vec::new();
        let mut next = unsafe { walker.GetFirstChildElementBuildCache(&el, request) };
        while let Ok(c) = next {
            next = unsafe { walker.GetNextSiblingElementBuildCache(&c, request) };
            kids.push(c);
        }
        for c in kids.into_iter().rev() {
            stack.push((c, Some(i), depth + 1));
        }
    }
    Ok(())
}

fn push(nodes: &mut Vec<Node>, el: IUIAutomationElement, parent: Option<usize>, depth: usize) -> usize {
    let s = |id| match cached(&el, id) {
        Val::Str(s) => s,
        _ => String::new(),
    };
    let b = |id| matches!(cached(&el, id), Val::Bool(true));
    let ob = |id| match cached(&el, id) {
        Val::Bool(v) => Some(v),
        _ => None,
    };
    let value = b(UIA_IsValuePatternAvailablePropertyId);
    let selectable = b(UIA_IsSelectionItemPatternAvailablePropertyId);
    let node = Node {
        parent,
        children: Vec::new(),
        depth,
        name: s(UIA_NamePropertyId),
        control_type: match cached(&el, UIA_ControlTypePropertyId) {
            Val::Int(v) => v,
            _ => 0,
        },
        automation_id: s(UIA_AutomationIdPropertyId),
        class_name: s(UIA_ClassNamePropertyId),
        aria_role: s(UIA_AriaRolePropertyId),
        framework_id: s(UIA_FrameworkIdPropertyId),
        provider: s(UIA_ProviderDescriptionPropertyId),
        rect: unsafe { el.CachedBoundingRectangle() }.unwrap_or_default(),
        offscreen: b(UIA_IsOffscreenPropertyId),
        enabled: b(UIA_IsEnabledPropertyId),
        focusable: b(UIA_IsKeyboardFocusablePropertyId),
        focused: b(UIA_HasKeyboardFocusPropertyId),
        invoke: b(UIA_IsInvokePatternAvailablePropertyId),
        value,
        value_read_only: if value { ob(UIA_ValueIsReadOnlyPropertyId) } else { None },
        value_len: if value {
            match cached(&el, UIA_ValueValuePropertyId) {
                Val::Str(s) => Some(s.chars().count()),
                _ => None,
            }
        } else {
            None
        },
        text: b(UIA_IsTextPatternAvailablePropertyId),
        legacy_action: s(UIA_LegacyIAccessibleDefaultActionPropertyId),
        selected: if selectable { ob(UIA_SelectionItemIsSelectedPropertyId) } else { None },
        el,
    };
    let i = nodes.len();
    nodes.push(node);
    if let Some(p) = parent {
        nodes[p].children.push(i);
    }
    i
}

enum Val {
    Bool(bool),
    Int(i32),
    Str(String),
    Other,
}

fn cached(el: &IUIAutomationElement, id: UIA_PROPERTY_ID) -> Val {
    let Ok(mut v) = (unsafe { el.GetCachedPropertyValue(id) }) else {
        return Val::Other;
    };
    let out = unsafe {
        let inner = &v.Anonymous.Anonymous;
        if inner.vt == VT_BOOL {
            Val::Bool(inner.Anonymous.boolVal.0 != 0)
        } else if inner.vt == VT_I4 {
            Val::Int(inner.Anonymous.lVal)
        } else if inner.vt == VT_BSTR {
            Val::Str(inner.Anonymous.bstrVal.to_string())
        } else {
            Val::Other
        }
    };
    let _ = unsafe { VariantClear(&mut v) };
    out
}

/// Finds the elements whose Name is `name` and whose ControlType is `control_type`, the way a
/// product would look for one button, and how long that took.
pub fn find(
    uia: &IUIAutomation,
    hwnd: HWND,
    name: &str,
    control_type: i32,
) -> Result<(usize, Duration)> {
    let start = Instant::now();
    unsafe {
        let root = uia.ElementFromHandle(hwnd)?;
        let mut by_name = variant_str(name);
        let mut by_type = variant_i4(control_type);
        let condition = uia.CreateAndCondition(
            &uia.CreatePropertyCondition(UIA_NamePropertyId, &by_name)?,
            &uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &by_type)?,
        )?;
        let _ = VariantClear(&mut by_name);
        let _ = VariantClear(&mut by_type);
        let found = root.FindAll(TreeScope_Descendants, &condition)?;
        Ok((found.Length()? as usize, start.elapsed()))
    }
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

pub fn control_type_name(id: i32) -> String {
    const NAMES: &[&str] = &[
        "Button", "Calendar", "CheckBox", "ComboBox", "Edit", "Hyperlink", "Image", "ListItem",
        "List", "Menu", "MenuBar", "MenuItem", "ProgressBar", "RadioButton", "ScrollBar", "Slider",
        "Spinner", "StatusBar", "Tab", "TabItem", "Text", "ToolBar", "ToolTip", "Tree", "TreeItem",
        "Custom", "Group", "Thumb", "DataGrid", "DataItem", "Document", "SplitButton", "Window",
        "Pane", "Header", "HeaderItem", "Table", "TitleBar", "Separator", "SemanticZoom", "AppBar",
    ];
    usize::try_from(id - 50000)
        .ok()
        .and_then(|i| NAMES.get(i))
        .map(|n| n.to_string())
        .unwrap_or_else(|| format!("ControlType({id})"))
}

pub const BUTTON: i32 = 50000;
pub const EDIT: i32 = 50004;
pub const DOCUMENT: i32 = 50030;
