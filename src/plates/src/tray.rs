//! Plates' icon in Windows' notification area and in macOS' menu bar, the one place to reach
//! Plates from outside the rack, Plates' floating window, which has no taskbar or Dock entry.
//! Its menu brings the rack back into view, connects Plates to Claude Code or disconnects it,
//! turns starting at login on or off, and quits Plates.
//! On Windows, clicking the icon also brings the rack back, and right-clicking it opens the menu,
//! as Windows' own tray icons do. On macOS, clicking it opens the menu, as a menu bar item does.

use std::io::Cursor;

use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

use plates_core::install::Connection;

use crate::native;

/// The menu item that connects Plates to Claude Code, checked while it's connected.
const CONNECT: &str = "Connect to Claude Code";
/// The same item while Claude Code's settings hold Plates' hooks or task server from another copy
/// or version of Plates.
const UPDATE: &str = "Connect to Claude Code (update needed)";

/// What a choice in the menu, or a click on the icon, asks of the rack. A Plates started while
/// this one runs asks for `Show` too, through the receiver.
pub enum Command {
    Show,
    Connect,
    ToggleLogin,
    Quit,
}

pub struct Tray {
    // The icon leaves the tray when this drops.
    _icon: TrayIcon,
    connect: CheckMenuItem,
    login: CheckMenuItem,
}

impl Tray {
    /// Puts Plates' icon in the tray, and hands each choice to `commands`.
    /// Call it on GPUI's main thread once the app runs, since the tray takes its clicks from that
    /// thread's event loop.
    pub fn new(commands: async_channel::Sender<Command>) -> Result<Tray, String> {
        let show = MenuItem::new("Show Plates", true, None);
        // The rack reads whether Plates is connected away from the main thread, and checks this
        // once it knows.
        let connect = CheckMenuItem::new(CONNECT, true, false, None);
        let login = CheckMenuItem::new("Start at login", true, native::starts_at_login(), None);
        let quit = MenuItem::new("Quit Plates", true, None);
        let menu = Menu::with_items(&[
            &show,
            &connect,
            &login,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .map_err(|e| e.to_string())?;

        let ids = (
            show.id().clone(),
            connect.id().clone(),
            login.id().clone(),
            quit.id().clone(),
        );
        let choices = commands.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let command = if event.id == ids.0 {
                Command::Show
            } else if event.id == ids.1 {
                Command::Connect
            } else if event.id == ids.2 {
                Command::ToggleLogin
            } else if event.id == ids.3 {
                Command::Quit
            } else {
                return;
            };
            let _ = choices.try_send(command);
        }));

        let builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Plates");
        // macOS tints a template image to suit the menu bar, light or dark, so the menu bar's
        // icon is a black shape rather than the app icon.
        #[cfg(target_os = "macos")]
        let builder = builder.with_icon_templated(icon(include_bytes!("../icons/menu-bar.png"))?);
        #[cfg(windows)]
        let builder = {
            use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
            TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    let _ = commands.try_send(Command::Show);
                }
            }));
            builder
                .with_icon(icon(include_bytes!("../icons/tray.png"))?)
                .with_menu_on_left_click(false)
        };
        let tray = builder.build().map_err(|e| e.to_string())?;
        Ok(Tray {
            _icon: tray,
            connect,
            login,
        })
    }

    /// Shows whether Plates is connected to Claude Code, or with `None`, that Plates couldn't
    /// tell. A click on a check item checks or unchecks it before the rack hears of the click, so
    /// the rack calls this again once you've answered the click's dialog.
    pub fn show_connection(&self, connection: Option<Connection>) {
        self.connect.set_checked(matches!(
            connection,
            Some(Connection::On | Connection::Stale)
        ));
        self.connect
            .set_text(if connection == Some(Connection::Stale) {
                UPDATE
            } else {
                CONNECT
            });
    }

    /// Shows whether Plates starts at login, after either menu changed it.
    pub fn show_login(&self, on: bool) {
        self.login.set_checked(on);
    }
}

// The icons are PNGs that tools/make-icons.mjs writes as 8-bit RGBA, the layout `Icon` takes.
fn icon(png: &[u8]) -> Result<Icon, String> {
    let mut reader = png::Decoder::new(Cursor::new(png))
        .read_info()
        .map_err(|e| e.to_string())?;
    let size = reader
        .output_buffer_size()
        .ok_or("the tray's icon is too large to decode")?;
    let mut rgba = vec![0; size];
    let info = reader.next_frame(&mut rgba).map_err(|e| e.to_string())?;
    rgba.truncate(info.buffer_size());
    Icon::from_rgba(rgba, info.width, info.height).map_err(|e| e.to_string())
}
