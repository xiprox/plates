//! Asking GitHub where each pull request a tile shows stands, through GitHub's `gh` command, on
//! a thread of its own, since an ask took 820ms to 960ms (measured on macOS 26.5.2, 2026-11-27,
//! record/areas/rack/log.md). Plates asks for an open pull request every minute, and for a merged
//! or closed one once, for its title (record/areas/rack/04-pull-requests.md).

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Stdio,
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use plates_core::pulls::{self, FIELDS, Pull, PullState};

use crate::program;

/// How often Plates asks again about an open pull request, or about one it couldn't read.
const AGAIN: Duration = Duration::from_secs(60);

/// The thread that asks `gh`, which runs until Plates quits.
pub struct Watcher {
    urls: mpsc::Sender<Vec<String>>,
}

impl Watcher {
    /// Starts the thread, which sends what it reads about each pull request, by its page.
    pub fn start() -> (Watcher, async_channel::Receiver<(String, Pull)>) {
        let (urls, wanted) = mpsc::channel::<Vec<String>>();
        let (found, results) = async_channel::unbounded();
        thread::Builder::new()
            .name("plates-pulls".into())
            .spawn(move || {
                let mut gh: Option<Option<PathBuf>> = None;
                let mut watched: Vec<String> = Vec::new();
                let mut asked: HashMap<String, (Instant, Option<PullState>)> = HashMap::new();
                let mut failed: HashSet<String> = HashSet::new();
                loop {
                    match wanted.recv_timeout(Duration::from_secs(1)) {
                        Ok(urls) => watched = wanted.try_iter().last().unwrap_or(urls),
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                    asked.retain(|url, _| watched.contains(url));
                    if watched.is_empty() {
                        continue;
                    }
                    // Finding `gh` can start your shell, so Plates looks once, when a tile first
                    // shows a pull request.
                    let Some(gh) = gh.get_or_insert_with(find_gh) else {
                        continue;
                    };
                    for url in &watched {
                        let due = match asked.get(url) {
                            None => true,
                            Some((at, Some(PullState::Open) | None)) => at.elapsed() >= AGAIN,
                            Some((_, Some(_))) => false,
                        };
                        if !due {
                            continue;
                        }
                        let read = view(gh, url);
                        asked.insert(
                            url.clone(),
                            (Instant::now(), read.as_ref().ok().map(|p| p.state)),
                        );
                        match read {
                            Ok(pull) => {
                                failed.remove(url);
                                if found.send_blocking((url.clone(), pull)).is_err() {
                                    return;
                                }
                            }
                            // Plates has no console, so this shows only when it runs from a
                            // terminal, once for each pull request until it reads again.
                            Err(e) if failed.insert(url.clone()) => {
                                eprintln!("Couldn't read {url} with gh: {e}");
                            }
                            Err(_) => {}
                        }
                    }
                }
            })
            .expect("the pull requests' thread should start");
        (Watcher { urls }, results)
    }

    /// Sets the pull requests to ask about, which replaces the ones before.
    pub fn watch(&self, urls: Vec<String>) {
        let _ = self.urls.send(urls);
    }
}

/// Where `gh` is: on `PATH`, or where Homebrew puts it on a Mac with Apple silicon or Intel
/// (documented in <https://docs.brew.sh/FAQ#why-should-i-install-homebrew-in-the-default-location>),
/// or on macOS, where your shell finds it.
fn find_gh() -> Option<PathBuf> {
    let homebrew: &[PathBuf] = if cfg!(target_os = "macos") {
        &[
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]
    } else {
        &[]
    };
    let found = program::find("gh", homebrew);
    if found.is_none() {
        eprintln!("Couldn't find GitHub's gh command, so tiles show no pull request's checks.");
    }
    found
}

fn view(gh: &Path, url: &str) -> Result<Pull, String> {
    let output = program::quiet(gh)
        .args(["pr", "view", url, "--json", FIELDS])
        .env("GH_PROMPT_DISABLED", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    pulls::parse(&String::from_utf8_lossy(&output.stdout))
}
