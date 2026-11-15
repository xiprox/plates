//! Probe: can a separate Windows program use UI Automation to press the Claude desktop app's
//! wrap-up buttons, or type into its composer, for one session?
//!
//! ```text
//! uia-answer [--session local_<id>] [--no-open] [--no-restore] [--out <report>] [--tree <file>]
//!            [--invoke <label>] [--set-text <text>]
//! ```
//!
//! Without `--invoke` or `--set-text` it only reads: it opens the session with
//! `claude://code/continue?session=local_<id>`, walks the app window's tree, and reports what an
//! invoke would hit. It never sends mouse or keyboard input. `--invoke` presses the reply button
//! of that label through InvokePattern, and `--set-text` puts text in the composer through
//! ValuePattern; both send something real, so they're for a throwaway session only.
//!
//! `--tree` writes every element of the last walk, message text included, so give it a path
//! outside the repository.

mod app;
mod uia;

use std::fmt::Write as _;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use uia::{BUTTON, DOCUMENT, EDIT, Tree, control_type_name as ctn};
use windows::Win32::UI::Accessibility::{
    IUIAutomation, IUIAutomationInvokePattern, IUIAutomationValuePattern, UIA_InvokePatternId,
    UIA_ValuePatternId, UiaClientsAreListening,
};
use windows::core::BSTR;

/// How many elements one walk keeps, so a runaway tree can't take the probe down.
const LIMIT: usize = 200_000;

#[derive(Default)]
struct Args {
    session: Option<String>,
    no_open: bool,
    no_restore: bool,
    out: Option<String>,
    tree: Option<String>,
    invoke: Option<String>,
    set_text: Option<String>,
}

fn args() -> Args {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--session" => a.session = it.next(),
            "--no-open" => a.no_open = true,
            "--no-restore" => a.no_restore = true,
            "--out" => a.out = it.next(),
            "--tree" => a.tree = it.next(),
            "--invoke" => a.invoke = it.next(),
            "--set-text" => a.set_text = it.next(),
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    a
}

#[derive(Default)]
struct Report(String);

impl Report {
    fn line(&mut self, s: impl AsRef<str>) {
        self.0.push_str(s.as_ref());
        self.0.push('\n');
    }
    fn head(&mut self, s: &str) {
        self.line("");
        self.line(format!("== {s} =="));
    }
}

fn main() {
    let a = args();
    let scrub = app::Scrub::new();
    let mut r = Report::default();
    let started = SystemTime::now();
    r.line(format!("uia-answer, run at {} UTC", utc(started)));
    r.line(format!("Windows: {}", windows_version()));

    // The session, from the app's file for it and its transcript.
    r.head("Session");
    let files = app::session_files();
    let showing_before = files
        .iter()
        .filter(|f| !f.archived && f.focused_at_ms > 0)
        .max_by_key(|f| f.focused_at_ms);
    match showing_before {
        Some(f) => r.line(format!(
            "session the app opened last (newest lastFocusedAt): {} \"{}\"",
            f.id, f.title
        )),
        None => r.line("session the app opened last: none found"),
    }
    let target = a.session.as_ref().and_then(|id| files.iter().find(|f| &f.id == id));
    let mut transcript = None;
    match (&a.session, target) {
        (None, _) => r.line("no --session given, so the probe reads whatever the app shows"),
        (Some(id), None) => r.line(format!("no session file has sessionId {id}")),
        (Some(_), Some(f)) => {
            r.line(format!("target: {} \"{}\", archived={}", f.id, f.title, f.archived));
            match app::transcript(&f.cli_id) {
                None => r.line(format!("no transcript found for cliSessionId {}", f.cli_id)),
                Some(path) => {
                    let t = app::read_transcript(&path);
                    match &t.wrap_up {
                        None => r.line("the transcript has no wrap-up call"),
                        Some(w) => {
                            r.line(format!("last wrap-up at {}, prompts typed since: {}", w.at, w.prompts_after));
                            r.line(format!("  ask: \"{}\"", w.ask));
                            r.line(format!("  reply labels: {:?}", w.labels));
                            r.line(format!("  app actions: {:?}", w.app_actions));
                            r.line(format!("  recap starts: \"{}\"", short(&w.recap, 40)));
                        }
                    }
                    r.line(format!("last assistant text starts: \"{}\"", short(&t.last_text, 40)));
                    transcript = Some(t);
                }
            }
        }
    }

    // What holds before this process creates a UI Automation client.
    r.head("Before the first walk");
    let listening = unsafe { UiaClientsAreListening() }.as_bool();
    r.line(format!("UiaClientsAreListening: {listening}"));
    for (switch, n) in app::accessibility_switches() {
        r.line(format!("app command lines, {switch}: {n}"));
    }
    let windows = app::app_windows();
    for w in &windows {
        r.line(format!(
            "window {:?} pid={} class=\"{}\" title=\"{}\" rect=({},{} {}x{}) app={}",
            w.hwnd.0,
            w.pid,
            w.class,
            w.title,
            w.rect.left,
            w.rect.top,
            w.rect.right - w.rect.left,
            w.rect.bottom - w.rect.top,
            app::app_version(w.pid)
        ));
    }
    let Some(main_window) = windows
        .iter()
        .filter(|w| w.class == "Chrome_WidgetWin_1")
        .max_by_key(|w| (w.rect.right - w.rect.left) as i64 * (w.rect.bottom - w.rect.top) as i64)
    else {
        r.line("no visible Chrome_WidgetWin_1 window of claude.exe, so nothing to walk");
        finish(&a, &scrub, &r, None);
        return;
    };
    let hwnd = main_window.hwnd;
    r.line(format!("walking window {:?}", hwnd.0));

    let uia = match uia::client() {
        Ok(u) => u,
        Err(e) => {
            r.line(format!("could not create the UI Automation client: {e}"));
            finish(&a, &scrub, &r, None);
            return;
        }
    };

    // Question 5: Chromium builds its tree only once a client asks, so walk until it settles.
    r.head("Question 5: the tree on the first walk and after");
    let t0 = Instant::now();
    let mut last: Option<Tree> = None;
    let schedule = [0u64, 300, 700, 1500, 3000, 6000, 10000];
    let mut counts: Vec<usize> = Vec::new();
    for at in schedule {
        let wait = Duration::from_millis(at).saturating_sub(t0.elapsed());
        std::thread::sleep(wait);
        let began = t0.elapsed();
        match uia::walk(&uia, hwnd, LIMIT) {
            Ok(t) => {
                r.line(format!("walk at +{}ms: {}", began.as_millis(), summary(&t)));
                counts.push(t.nodes.len());
                last = Some(t);
            }
            Err(e) => r.line(format!("walk at +{}ms failed: {e}", began.as_millis())),
        }
        let n = counts.len();
        if n >= 3 && counts[n - 1] == counts[n - 2] && counts[n - 1] > 50 {
            break;
        }
    }
    if let Some(t) = &last {
        for i in selected_items(t) {
            r.line(format!("selected item: {}", describe(t, i)));
        }
    }

    // Open the session, and wait for its last turn to show.
    let marks = transcript.as_ref().map(markers).unwrap_or_default();
    let mut opened = false;
    if let (Some(id), false) = (&a.session, a.no_open) {
        r.head("Opening the session");
        let url = format!("claude://code/continue?session={id}");
        let (fg, fg_app) = app::foreground();
        r.line(format!("foreground before: {:?}, the app's: {fg_app}", fg.0));
        let t1 = Instant::now();
        let ok = app::open_link(&url);
        opened = true;
        r.line(format!("ShellExecuteW {url}: {}", if ok { "ok" } else { "failed" }));
        let mut seen = None;
        while t1.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(500));
            let Ok(t) = uia::walk(&uia, hwnd, LIMIT) else {
                continue;
            };
            let found = find_markers(&t, &marks);
            let at = t1.elapsed();
            last = Some(t);
            if !found.is_empty() {
                seen = Some((at, found));
                break;
            }
        }
        match &seen {
            Some((at, found)) => {
                r.line(format!("the session's last turn showed within {}ms, by {:?}", at.as_millis(), found))
            }
            None if marks.is_empty() => r.line("no marker to wait for, so the last walk is from 20s after the link"),
            None => r.line(format!("no marker showed within 20s: {:?}", marks.iter().map(|m| m.0).collect::<Vec<_>>())),
        }
        let (fg, fg_app) = app::foreground();
        r.line(format!("foreground after: {:?}, the app's: {fg_app}", fg.0));
        if let Some(t) = &last {
            r.line(format!("walk after opening: {}", summary(t)));
        }
    }

    let Some(tree) = last else {
        r.line("no walk succeeded");
        finish(&a, &scrub, &r, None);
        return;
    };
    let marker_nodes: Vec<usize> = marks
        .iter()
        .flat_map(|(_, m)| nodes_containing(&tree, m))
        .collect();

    // Question 3: the wrap-up's buttons.
    r.head("Question 3: the wrap-up's buttons");
    let wrap = transcript.as_ref().and_then(|t| t.wrap_up.as_ref());
    let mut wrap_nodes: Vec<usize> = Vec::new();
    match wrap {
        None => r.line("no wrap-up in the transcript, so no labels to look for"),
        Some(w) => {
            for label in &w.labels {
                let exact: Vec<usize> = (0..tree.nodes.len())
                    .filter(|&i| tree.nodes[i].name.trim() == label.trim())
                    .collect();
                r.line(format!("label \"{label}\": {} element(s) named exactly so", exact.len()));
                for &i in exact.iter().take(5) {
                    element(&mut r, &tree, i);
                    wrap_nodes.push(i);
                }
                if exact.is_empty() {
                    let lower = label.to_lowercase();
                    for i in (0..tree.nodes.len())
                        .filter(|&i| tree.nodes[i].name.to_lowercase().contains(&lower))
                        .take(5)
                    {
                        r.line(format!("  contains it: {}", describe(&tree, i)));
                    }
                }
                if let Ok((n, took)) = uia::find(&uia, hwnd, label, BUTTON) {
                    r.line(format!(
                        "  FindAll(Descendants, Name=\"{label}\" and ControlType=Button): {n} in {}ms",
                        took.as_millis()
                    ));
                }
            }
            if !w.ask.trim().is_empty() {
                let asks = nodes_containing(&tree, w.ask.trim());
                r.line(format!("elements whose Name holds the ask: {}", asks.len()));
                for &i in asks.iter().take(3) {
                    element(&mut r, &tree, i);
                    wrap_nodes.push(i);
                }
            }
        }
    }
    if wrap_nodes.len() >= 2 {
        let c = wrap_nodes[1..].iter().fold(wrap_nodes[0], |acc, &i| lca(&tree, acc, i));
        r.line("the smallest element holding all of them:");
        element(&mut r, &tree, c);
        r.line("the invokable elements inside it:");
        for i in subtree(&tree, c).into_iter().filter(|&i| tree.nodes[i].invoke).take(40) {
            r.line(format!("  {}", describe(&tree, i)));
        }
    }

    // Question 4: the composer.
    r.head("Question 4: the composer");
    let candidates: Vec<usize> = (0..tree.nodes.len())
        .filter(|&i| {
            let n = &tree.nodes[i];
            (n.control_type == EDIT || n.control_type == DOCUMENT)
                && (n.value || n.text)
                && n.focusable
                && n.value_read_only != Some(true)
        })
        .collect();
    r.line(format!(
        "editable candidates (Edit or Document, with Value or Text, focusable, not read-only): {}",
        candidates.len()
    ));
    for &i in candidates.iter().take(8) {
        element(&mut r, &tree, i);
        if let Some(&m) = marker_nodes.first() {
            r.line(format!(
                "  shares an ancestor with the session's last turn at depth {} (the candidate is at {})",
                tree.nodes[lca(&tree, i, m)].depth,
                tree.nodes[i].depth
            ));
        }
        let n = &tree.nodes[i];
        if let Ok((count, took)) = uia::find(&uia, hwnd, &n.name, n.control_type) {
            r.line(format!(
                "  FindAll(Descendants, Name=\"{}\" and ControlType={}): {count} in {}ms",
                n.name,
                ctn(n.control_type),
                took.as_millis()
            ));
        }
    }
    let composer = candidates
        .iter()
        .copied()
        .max_by_key(|&i| marker_nodes.first().map(|&m| tree.nodes[lca(&tree, i, m)].depth).unwrap_or(0));
    let edits: Vec<usize> = (0..tree.nodes.len())
        .filter(|&i| tree.nodes[i].control_type == EDIT)
        .collect();
    r.line(format!("every Edit element: {}", edits.len()));
    for &i in edits.iter().take(20) {
        r.line(format!("  {}", describe(&tree, i)));
    }
    if let Some(c) = composer {
        let mut up = c;
        for _ in 0..4 {
            up = tree.nodes[up].parent.unwrap_or(up);
        }
        r.line("the invokable elements within four levels above the composer:");
        for i in subtree(&tree, up).into_iter().filter(|&i| tree.nodes[i].invoke).take(30) {
            r.line(format!("  {}", describe(&tree, i)));
        }
    }

    // The actions, which only run when asked for by name.
    if let Some(label) = &a.invoke {
        r.head("Invoking a reply button");
        let hits: Vec<usize> = (0..tree.nodes.len())
            .filter(|&i| tree.nodes[i].control_type == BUTTON && tree.nodes[i].name.trim() == label.trim())
            .collect();
        if hits.len() != 1 {
            r.line(format!("{} buttons are named \"{label}\", so the probe invokes none", hits.len()));
        } else {
            let el = &tree.nodes[hits[0]].el;
            let result = unsafe {
                el.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
                    .and_then(|p| p.Invoke())
            };
            r.line(format!("Invoke on \"{label}\": {result:?}"));
            after_action(&mut r, &uia, hwnd, &marks, target.map(|f| f.cli_id.as_str()));
        }
    }
    if let (Some(text), Some(c)) = (&a.set_text, composer) {
        r.head("Setting the composer's text");
        let el = &tree.nodes[c].el;
        let result = unsafe {
            el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .and_then(|p| p.SetValue(&BSTR::from(text.as_str())))
        };
        r.line(format!("SetValue on {}: {result:?}", describe(&tree, c)));
        std::thread::sleep(Duration::from_millis(500));
        let now = unsafe {
            el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .and_then(|p| p.CurrentValue())
        };
        match now {
            Ok(v) => r.line(format!("its value 500ms later equals the text set: {}", v.to_string() == *text)),
            Err(e) => r.line(format!("reading its value back failed: {e}")),
        }
    }

    // Put back the session the app showed, so the probe leaves the app as it found it.
    if opened && !a.no_restore {
        if let Some(prev) = showing_before.filter(|p| Some(&p.id) != a.session.as_ref()) {
            r.head("Restoring the session the app showed");
            let ok = app::open_link(&format!("claude://code/continue?session={}", prev.id));
            r.line(format!("opened {} \"{}\": {}", prev.id, prev.title, if ok { "ok" } else { "failed" }));
        }
    }

    finish(&a, &scrub, &r, Some(&tree));
}

fn after_action(r: &mut Report, uia: &IUIAutomation, hwnd: windows::Win32::Foundation::HWND, marks: &[(&'static str, String)], cli_id: Option<&str>) {
    std::thread::sleep(Duration::from_secs(3));
    if let Ok(t) = uia::walk(uia, hwnd, LIMIT) {
        r.line(format!("3s later the markers still showing: {:?}", find_markers(&t, marks)));
    }
    if let Some(path) = cli_id.and_then(app::transcript) {
        let t = app::read_transcript(&path);
        r.line(format!(
            "3s later the transcript's last wrap-up has {} typed prompts after it",
            t.wrap_up.map(|w| w.prompts_after).unwrap_or(0)
        ));
    }
}

fn finish(a: &Args, scrub: &app::Scrub, r: &Report, tree: Option<&Tree>) {
    let text = scrub.apply(&r.0);
    print!("{text}");
    if let Some(out) = &a.out {
        if let Err(e) = std::fs::write(out, &text) {
            eprintln!("could not write {out}: {e}");
        }
    }
    if let (Some(path), Some(t)) = (&a.tree, tree) {
        let mut s = String::new();
        for (i, n) in t.nodes.iter().enumerate() {
            let _ = writeln!(s, "{}{} #{i}", "  ".repeat(n.depth.min(80)), describe(t, i));
        }
        if let Err(e) = std::fs::write(path, scrub.apply(&s)) {
            eprintln!("could not write {path}: {e}");
        }
    }
}

/// Text that shows only once the session's last turn is on screen.
fn markers(t: &app::Transcript) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if let Some(w) = &t.wrap_up {
        if !w.ask.trim().is_empty() {
            out.push(("wrap-up ask", w.ask.trim().to_string()));
        }
        let recap: String = w.recap.trim().chars().take(40).collect();
        if !recap.is_empty() {
            out.push(("wrap-up recap", recap));
        }
    }
    let first = t.last_text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let plain: String = first.chars().filter(|c| !"*`#_[]".contains(*c)).take(40).collect();
    if plain.trim().len() >= 10 {
        out.push(("last message", plain.trim().to_string()));
    }
    out
}

fn find_markers(t: &Tree, marks: &[(&'static str, String)]) -> Vec<&'static str> {
    marks
        .iter()
        .filter(|(_, m)| t.nodes.iter().any(|n| n.name.contains(m.as_str())))
        .map(|(k, _)| *k)
        .collect()
}

fn nodes_containing(t: &Tree, text: &str) -> Vec<usize> {
    (0..t.nodes.len()).filter(|&i| t.nodes[i].name.contains(text)).collect()
}

fn selected_items(t: &Tree) -> Vec<usize> {
    (0..t.nodes.len()).filter(|&i| t.nodes[i].selected == Some(true)).take(10).collect()
}

fn summary(t: &Tree) -> String {
    let depth = t.nodes.iter().map(|n| n.depth).max().unwrap_or(0);
    let chrome = t.nodes.iter().filter(|n| n.framework_id == "Chrome").count();
    let docs = t.nodes.iter().filter(|n| n.control_type == DOCUMENT).count();
    let buttons = t.nodes.iter().filter(|n| n.control_type == BUTTON).count();
    let doc_kids: Vec<usize> = t
        .nodes
        .iter()
        .filter(|n| n.control_type == DOCUMENT)
        .map(|n| n.children.len())
        .collect();
    format!(
        "{} elements, depth {depth}, {chrome} with FrameworkId Chrome, {buttons} buttons, {docs} documents with {:?} children, took {}ms by {}",
        t.nodes.len(),
        doc_kids,
        t.took.as_millis(),
        t.method
    )
}

/// One element, with how to find it again.
fn element(r: &mut Report, t: &Tree, i: usize) {
    r.line(format!("  {}", describe(t, i)));
    r.line(format!("    provider: {}", t.nodes[i].provider));
    r.line(format!("    path: {}", path(t, i)));
    let named = named_ancestors(t, i);
    if !named.is_empty() {
        r.line("    named ancestors, outermost first:");
        for a in named {
            r.line(format!("      {a}"));
        }
    }
    r.line(format!("    selectors: {}", selectors(t, i).join("; ")));
}

fn describe(t: &Tree, i: usize) -> String {
    let n = &t.nodes[i];
    let mut pats = Vec::new();
    if n.invoke {
        pats.push("Invoke".to_string());
    }
    if n.value {
        pats.push(format!(
            "Value(readOnly={}, length={})",
            n.value_read_only.map(|b| b.to_string()).unwrap_or("?".into()),
            n.value_len.map(|l| l.to_string()).unwrap_or("?".into())
        ));
    }
    if n.text {
        pats.push("Text".to_string());
    }
    if let Some(s) = n.selected {
        pats.push(format!("SelectionItem(selected={s})"));
    }
    format!(
        "{} name=\"{}\" automationId=\"{}\" className=\"{}\" ariaRole=\"{}\" framework=\"{}\" patterns=[{}] enabled={} offscreen={} focusable={} focused={} rect=({},{} {}x{}) defaultAction=\"{}\" children={}",
        ctn(n.control_type),
        short(&n.name, 100),
        n.automation_id,
        short(&n.class_name, 80),
        n.aria_role,
        n.framework_id,
        pats.join(", "),
        n.enabled,
        n.offscreen,
        n.focusable,
        n.focused,
        n.rect.left,
        n.rect.top,
        n.rect.right - n.rect.left,
        n.rect.bottom - n.rect.top,
        n.legacy_action,
        n.children.len()
    )
}

/// The element's place in the tree, as each step's ControlType and its index among siblings of
/// that type, as in `Window[0]/Pane[1]/Document[0]`.
fn path(t: &Tree, i: usize) -> String {
    let mut parts = Vec::new();
    let mut cur = i;
    while let Some(p) = t.nodes[cur].parent {
        let ct = t.nodes[cur].control_type;
        let k = t.nodes[p]
            .children
            .iter()
            .take_while(|&&c| c != cur)
            .filter(|&&c| t.nodes[c].control_type == ct)
            .count();
        parts.push(format!("{}[{k}]", ctn(ct)));
        cur = p;
    }
    parts.push(ctn(t.nodes[cur].control_type));
    parts.reverse();
    parts.join("/")
}

fn named_ancestors(t: &Tree, i: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = t.nodes[i].parent;
    while let Some(p) = cur {
        let n = &t.nodes[p];
        if !n.name.trim().is_empty() || !n.automation_id.is_empty() {
            out.push(format!(
                "{} name=\"{}\" automationId=\"{}\" ariaRole=\"{}\" depth={}",
                ctn(n.control_type),
                short(&n.name, 60),
                n.automation_id,
                n.aria_role,
                n.depth
            ));
        }
        cur = n.parent;
    }
    out.reverse();
    out
}

/// How many elements in the tree share each property the element could be found by.
fn selectors(t: &Tree, i: usize) -> Vec<String> {
    let n = &t.nodes[i];
    let count = |f: &dyn Fn(&uia::Node) -> bool| t.nodes.iter().filter(|m| f(m)).count();
    let mut out = Vec::new();
    if n.automation_id.is_empty() {
        out.push("AutomationId empty".to_string());
    } else {
        out.push(format!(
            "AutomationId shared by {}",
            count(&|m| m.automation_id == n.automation_id)
        ));
    }
    out.push(format!(
        "Name+ControlType shared by {}",
        count(&|m| m.name == n.name && m.control_type == n.control_type)
    ));
    if !n.class_name.is_empty() {
        out.push(format!(
            "ClassName+ControlType shared by {}",
            count(&|m| m.class_name == n.class_name && m.control_type == n.control_type)
        ));
    }
    if !n.aria_role.is_empty() {
        out.push(format!(
            "AriaRole \"{}\" shared by {}",
            n.aria_role,
            count(&|m| m.aria_role == n.aria_role)
        ));
    }
    out
}

fn ancestors(t: &Tree, i: usize) -> Vec<usize> {
    let mut out = vec![i];
    let mut cur = i;
    while let Some(p) = t.nodes[cur].parent {
        out.push(p);
        cur = p;
    }
    out
}

fn lca(t: &Tree, a: usize, b: usize) -> usize {
    let up = ancestors(t, a);
    ancestors(t, b).into_iter().find(|x| up.contains(x)).unwrap_or(0)
}

fn subtree(t: &Tree, i: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut stack = vec![i];
    while let Some(n) = stack.pop() {
        out.push(n);
        stack.extend(t.nodes[n].children.iter().rev());
    }
    out
}

fn short(s: &str, n: usize) -> String {
    let flat = s.replace(['\r', '\n'], " ");
    if flat.chars().count() <= n {
        flat
    } else {
        format!("{}...", flat.chars().take(n).collect::<String>())
    }
}

fn windows_version() -> String {
    std::process::Command::new("cmd.exe")
        .args(["/c", "ver"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// A wall-clock time as `YYYY-MM-DD HH:MM:SS`, in UTC.
fn utc(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Howard Hinnant's days-to-civil algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}
