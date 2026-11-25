//! Plates updating itself from its GitHub releases through Velopack, as
//! record/areas/build/01-updates.md designs it: it looks for a newer release, downloads it in the
//! background, and installs it when you click the rack's menu row for it.

use std::time::Duration;

use gpui_kit::*;
use velopack::{UpdateCheck, UpdateManager, VelopackAsset, sources::GithubSource};

use crate::{connect, native};

/// The repository whose releases Plates updates from.
const REPOSITORY: &str = "https://github.com/xiprox/plates";

/// How long after Plates starts it first looks, so the look doesn't compete with the rack opening.
const FIRST_LOOK: Duration = Duration::from_secs(30);

/// How often Plates looks again. A look spends a request or a few against the limit GitHub's API
/// puts on an address without a token, so a look every 6 hours stays far inside it
/// (record/areas/build/01-updates.md).
const LOOK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// How long a look and a download may take. Velopack's requests have no time limit of their own,
/// so without these a request that stalls, as after a laptop sleeps or changes networks, would
/// leave the row at "Checking for updates…" or "Downloading…" until Plates restarts.
const LOOK_LIMIT: Duration = Duration::from_secs(2 * 60);
const DOWNLOAD_LIMIT: Duration = Duration::from_secs(30 * 60);

/// Where an update stands, which the rack's menu row for updates says in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// This copy sits outside Velopack's layout, such as a build run from target\release, so
    /// Velopack finds no manifest beside it to update.
    NotInstalled,
    /// No look has finished yet.
    Idle,
    Checking,
    UpToDate,
    Failed,
    /// Downloading the version named, such as "0.6.0".
    Downloading(String),
    /// The version named is downloaded, and installs when you click it.
    Ready(String),
    Installing,
}

/// Velopack's updater has started and waits for Plates to quit, which the rack does when it hears
/// this.
pub struct ReadyToQuit;

pub struct Updates {
    manager: Option<UpdateManager>,
    /// The version that runs, such as "0.5.0".
    pub version: String,
    pub state: State,
    /// The downloaded package, while the state is `Ready`.
    ready: Option<VelopackAsset>,
}

impl EventEmitter<ReadyToQuit> for Updates {}

impl Updates {
    /// `looks` is false for the showcase, whose made-up rack has nothing to update.
    pub fn new(looks: bool, cx: &mut Context<Self>) -> Self {
        let manager = looks
            .then(|| UpdateManager::new(GithubSource::new(REPOSITORY, None, false), None, None))
            .and_then(|made| match made {
                Ok(manager) => Some(manager),
                Err(velopack::Error::NotInstalled(_)) => None,
                Err(e) => {
                    eprintln!("Couldn't set up updates: {e}");
                    None
                }
            });
        let version = manager.as_ref().map_or_else(
            || env!("CARGO_PKG_VERSION").to_string(),
            UpdateManager::get_current_version_as_string,
        );
        let ready = manager
            .as_ref()
            .and_then(UpdateManager::get_update_pending_restart);
        let state = match (&manager, &ready) {
            (None, _) => State::NotInstalled,
            (Some(_), Some(asset)) => State::Ready(asset.Version.clone()),
            (Some(_), None) => State::Idle,
        };
        if manager.is_some() {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(FIRST_LOOK).await;
                while this.update(cx, |this, cx| this.check(cx)).is_ok() {
                    cx.background_executor().timer(LOOK_EVERY).await;
                }
            })
            .detach();
        }
        Updates {
            manager,
            version,
            state,
            ready,
        }
    }

    /// Whether an update is downloaded and waits for a click, which the grip's teal dot shows.
    pub fn is_ready(&self) -> bool {
        matches!(self.state, State::Ready(_))
    }

    /// Looks for a newer release, and downloads one it finds. While a look runs, or an update
    /// downloads, waits, or installs, it does nothing.
    pub fn check(&mut self, cx: &mut Context<Self>) {
        let Some(manager) = self.manager.clone() else {
            return;
        };
        if !matches!(self.state, State::Idle | State::UpToDate | State::Failed) {
            return;
        }
        self.settle(State::Checking, None, cx);
        cx.spawn(async move |this, cx| {
            // Velopack's calls block on the network, so they run off the UI thread. A call that
            // runs out of time keeps its thread until its request ends, and the look fails.
            let looked = {
                let manager = manager.clone();
                cx.background_executor()
                    .spawn(async move { manager.check_for_updates() })
                    .with_timeout(LOOK_LIMIT, cx.background_executor())
                    .await
            };
            let info = match looked {
                Ok(Ok(UpdateCheck::UpdateAvailable(info))) => info,
                Ok(Ok(UpdateCheck::NoUpdateAvailable | UpdateCheck::RemoteIsEmpty)) => {
                    this.update(cx, |this, cx| this.settle(State::UpToDate, None, cx))
                        .ok();
                    return;
                }
                Ok(Err(e)) => {
                    eprintln!("Couldn't look for an update: {e}");
                    this.update(cx, |this, cx| this.settle(State::Failed, None, cx))
                        .ok();
                    return;
                }
                Err(_) => {
                    eprintln!("Looking for an update took longer than {LOOK_LIMIT:?}.");
                    this.update(cx, |this, cx| this.settle(State::Failed, None, cx))
                        .ok();
                    return;
                }
            };
            let version = info.TargetFullRelease.Version.clone();
            let downloading = State::Downloading(version.clone());
            this.update(cx, |this, cx| this.settle(downloading, None, cx))
                .ok();
            let downloaded = cx
                .background_executor()
                .spawn(async move {
                    manager
                        .download_updates(&info, None)
                        .map(|()| info.TargetFullRelease.clone())
                })
                .with_timeout(DOWNLOAD_LIMIT, cx.background_executor())
                .await;
            this.update(cx, |this, cx| match downloaded {
                Ok(Ok(asset)) => this.settle(State::Ready(version), Some(asset), cx),
                Ok(Err(e)) => {
                    eprintln!("Couldn't download Plates {version}: {e}");
                    this.settle(State::Failed, None, cx);
                }
                Err(_) => {
                    eprintln!("Downloading Plates {version} took longer than {DOWNLOAD_LIMIT:?}.");
                    this.settle(State::Failed, None, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Starts Velopack's updater on the ready update, which waits for Plates to quit, installs
    /// the update, and starts Plates again. Once it has started, this stops the task servers and
    /// tells the rack to quit.
    pub fn install(&mut self, cx: &mut Context<Self>) {
        let (Some(manager), Some(asset)) = (self.manager.clone(), self.ready.clone()) else {
            return;
        };
        self.state = State::Installing;
        cx.notify();
        // Velopack starts its updater with CreateProcessW, which can pump the window's messages
        // and let GPUI run work while the click still holds its state, as ShellExecuteW did on
        // the rack's thread (record/areas/rack/log.md, 2026-11-15), so it starts off the click.
        cx.spawn(async move |this, cx| {
            let started = cx
                .background_executor()
                .spawn(async move {
                    manager.wait_exit_then_apply_updates(&asset, false, true, Vec::<String>::new())
                })
                .await;
            this.update(cx, |this, cx| match started {
                Ok(()) => {
                    if let Ok(server) = connect::beside("plates-tasks") {
                        native::stop_task_servers(&server);
                    }
                    cx.emit(ReadyToQuit);
                }
                Err(e) => {
                    eprintln!("Couldn't start the update: {e}");
                    this.settle(State::Failed, None, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn settle(&mut self, state: State, ready: Option<VelopackAsset>, cx: &mut Context<Self>) {
        self.state = state;
        self.ready = ready;
        cx.notify();
    }
}
