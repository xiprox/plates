# 001: Plates is written in Rust, with GPUI for its interface

Plates is written in Rust, and its interface is drawn with GPUI, the GPU-rendered UI framework Zed is built on.
Decided by the user on 2026-11-15, after the mockup review.

## Context

Plates runs all day on Windows and macOS, so it has to stay light, and the user wants it in Rust throughout, with no webview (user, 2026-11-15).
Its interface is drawn by hand: tiles, a timer ring, and one-time color changes, with no stock widgets.
The risky part is the window [01-window](../areas/rack/01-window.md) designs: staying on top, having no taskbar entry, taking clicks without taking focus, and showing on every macOS Space, which any of the frameworks may leave partly to each OS's own API (unverified).

## Options

1. **GPUI.**
   The user has built an app with it before, choom-collector.
   There, its window used 0.15% to 0.6% of one core while idle, with a status line updating every second (measured on Windows 11, 2026-11-13, in the choom-collector repository, record/areas/interface/log.md, entry "2026-11-13: gpui-kit builds every control the window needs on Windows, and the exe is 16.65 MB").
   Its own window options don't cover everything the rack needs, so the rest would be calls into the Windows and macOS APIs through the window's native handle (unverified, until the window test tries it).
   It ships as `gpui-pre`, a weekly snapshot of Zed's gpui, and `gpui-kit`'s README describes 0.7 while the crate is 0.9.1, so we pin exact versions (same log entry).
2. **Slint.**
   A Rust UI framework with a stable, documented API, its own markup language, built-in animation and text input, and access to the native window.
   It costs a markup language to learn, and its look is harder to draw entirely by hand.
3. **Tauri.**
   Rust, with a webview drawing the interface.
   It fails the user's wish for Rust throughout.

## Decision

Option 1.

## What it commits us to

- [The plan's step 1](../areas/product/plan.md#1-window-test) tries GPUI against the rack's window on both OSes before the rack is built.
  If GPUI can't give the rack that window, a decision for Slint reverses this one.
- The `plates-core` crate holds no GPUI code, so the interface can be replaced without touching the model, the timers, saving, or the hook receiver.
