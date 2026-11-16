// Reads the Claude desktop app's Accessibility tree on macOS, to find and press a session's
// wrap-up buttons, the macOS counterpart of uia-answer.
//
// macOS grants Accessibility to the process responsible for a program, which for a program started
// from a shell is the shell's app, so run.sh wraps this in AxProbe.app and starts it with `open`.
//
//   tree [depth]            every element to that depth, with role, subrole, title, description
//   find <needle>           elements whose title, description, or value holds the needle, and
//                           the buttons under them
//   press <title> <choice>  presses the button labeled <choice> in the "Suggested next actions"
//                           group of the web area titled "<title> - Claude Code", timing each step
import AppKit
import ApplicationServices

let args = Array(CommandLine.arguments.dropFirst())
let mode = args.first ?? "tree"

let prompt = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
print("trusted: \(AXIsProcessTrustedWithOptions(prompt))")
guard
  let app = NSRunningApplication.runningApplications(
    withBundleIdentifier: "com.anthropic.claudefordesktop"
  ).first
else {
  print("Claude isn't running")
  exit(1)
}
let root = AXUIElementCreateApplication(app.processIdentifier)

func attr(_ e: AXUIElement, _ name: String) -> AnyObject? {
  var v: AnyObject?
  return AXUIElementCopyAttributeValue(e, name as CFString, &v) == .success ? v : nil
}
func str(_ e: AXUIElement, _ name: String) -> String { (attr(e, name) as? String) ?? "" }
func kids(_ e: AXUIElement) -> [AXUIElement] { (attr(e, "AXChildren") as? [AXUIElement]) ?? [] }
func line(_ e: AXUIElement) -> String {
  let value = (attr(e, "AXValue") as? String).map { " value=\(String($0.prefix(50)))" } ?? ""
  return
    "\(str(e, "AXRole"))/\(str(e, "AXSubrole")) title=\(str(e, "AXTitle")) desc=\(str(e, "AXDescription"))\(value)"
}

// Electron builds its tree only for an assistive app that asks for it.
var t = Date()
let manual = AXUIElementSetAttributeValue(root, "AXManualAccessibility" as CFString, kCFBooleanTrue)
print("AXManualAccessibility set: \(manual.rawValue) in \(ms(t))")
func ms(_ since: Date) -> String { String(format: "%.0fms", Date().timeIntervalSince(since) * 1000) }

var visited = 0
func walk(_ e: AXUIElement, _ depth: Int, _ maxDepth: Int, _ visit: (AXUIElement, Int) -> Bool) {
  visited += 1
  guard visit(e, depth), depth < maxDepth else { return }
  for k in kids(e) { walk(k, depth + 1, maxDepth, visit) }
}

// The first element under `root`, depth first, that `match` accepts.
func first(_ root: AXUIElement, _ match: (AXUIElement) -> Bool) -> AXUIElement? {
  if match(root) { return root }
  for k in kids(root) { if let f = first(k, match) { return f } }
  return nil
}

switch mode {
case "tree":
  let maxDepth = args.count > 1 ? Int(args[1]) ?? 12 : 12
  Thread.sleep(forTimeInterval: 1.0)
  t = Date()
  walk(root, 0, maxDepth) { e, d in
    print(String(repeating: " ", count: d) + "\(d) " + line(e))
    return true
  }
  print("visited \(visited) in \(ms(t))")
case "find":
  let needle = args.count > 1 ? args[1] : "Suggested"
  Thread.sleep(forTimeInterval: 1.0)
  t = Date()
  func find(_ e: AXUIElement, _ d: Int, _ under: Bool) {
    visited += 1
    let hit = [str(e, "AXTitle"), str(e, "AXDescription"), (attr(e, "AXValue") as? String) ?? ""]
      .contains { $0.contains(needle) }
    if hit || (under && str(e, "AXRole") == "AXButton") {
      print(String(repeating: " ", count: d) + "\(d) " + line(e))
    }
    for k in kids(e) { find(k, d + 1, under || hit) }
  }
  find(root, 0, false)
  print("visited \(visited) in \(ms(t))")
case "press":
  guard args.count > 2 else {
    print("usage: press <title> <choice>")
    exit(2)
  }
  let document = "\(args[1]) - Claude Code"
  let deadline = Date().addingTimeInterval(5)
  let start = Date()
  while true {
    t = Date()
    let windows = (attr(root, "AXWindows") as? [AXUIElement]) ?? []
    var found: AXUIElement?
    for w in windows {
      guard
        let page = first(w, { str($0, "AXRole") == "AXWebArea" && str($0, "AXTitle") == document }),
        let group = first(page, { str($0, "AXDescription") == "Suggested next actions" || str($0, "AXTitle") == "Suggested next actions" }),
        let button = first(group, { str($0, "AXRole") == "AXButton" && (str($0, "AXTitle") == args[2] || str($0, "AXDescription") == args[2]) })
      else { continue }
      found = button
      break
    }
    print("search over \(windows.count) windows took \(ms(t)), found: \(found != nil)")
    if let b = found {
      print("button: \(line(b)) enabled=\(attr(b, "AXEnabled") as? Bool ?? false)")
      t = Date()
      let r = AXUIElementPerformAction(b, kAXPressAction as CFString)
      print("AXPress: \(r.rawValue) in \(ms(t)), \(ms(start)) after start")
      break
    }
    if Date() > deadline {
      print("gave up after \(ms(start))")
      break
    }
    Thread.sleep(forTimeInterval: 0.1)
  }
default:
  print("unknown mode \(mode)")
}
