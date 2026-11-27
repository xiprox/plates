//! A session's pull requests, which its tile shows as a chip at the end of its title: the ones
//! the Claude desktop app links to the session, and what GitHub says about each, which Plates
//! asks `gh` for (record/areas/rack/04-pull-requests.md).

use serde_json::Value;

/// A pull request the Claude desktop app links to a session, from the `prs` list in its file for
/// the session, which runs in the order the app linked them (unverified,
/// record/areas/hooks/log.md, 2026-11-27).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullLink {
    /// The pull request's page, such as `https://github.com/xiprox/plates/pull/8`.
    pub url: String,
    /// The repository as `owner/name`.
    pub repo: String,
    pub number: u64,
    /// Whether the app last saw it open, rather than merged or closed, which holds until `gh`
    /// says otherwise.
    pub open: bool,
    /// Whether you dismissed the pull request's card in the app, which leaves it off the tile too.
    pub dismissed: bool,
}

/// Reads a session file's `prs` list, skipping any entry without a number, a repository, and a
/// page on GitHub.
pub fn links(prs: Option<&Value>) -> Vec<PullLink> {
    let Some(prs) = prs.and_then(Value::as_array) else {
        return Vec::new();
    };
    prs.iter()
        .filter_map(|p| {
            let text = |key: &str| p.get(key).and_then(Value::as_str).map(str::to_string);
            let url = text("url").filter(|u| u.starts_with("https://github.com/"))?;
            Some(PullLink {
                url,
                repo: text("repo")?,
                number: p.get("prNumber").and_then(Value::as_u64)?,
                open: text("state").as_deref() == Some("OPEN"),
                dismissed: p.get("dismissed").and_then(Value::as_bool) == Some(true),
            })
        })
        .collect()
}

/// A pull request from its page on GitHub, `https://github.com/<owner>/<name>/pull/<number>`,
/// with anything after the number left off, as in a link to its files or a comment. Until GitHub
/// says otherwise, it counts as open.
pub fn from_url(url: &str) -> Option<PullLink> {
    let rest = url.trim().strip_prefix("https://github.com/")?;
    let mut parts = rest.split(['/', '?', '#']);
    let (owner, name, pull, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let number: u64 = number.parse().ok()?;
    let plain = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (pull == "pull" && plain(owner) && plain(name)).then(|| PullLink {
        url: format!("https://github.com/{owner}/{name}/pull/{number}"),
        repo: format!("{owner}/{name}"),
        number,
        open: true,
        dismissed: false,
    })
}

/// A session's pull requests: the ones the Claude desktop app links to it, then the ones Claude
/// linked with the `plates` server's `pr_link`, by their pages, leaving out any the app links too.
pub fn combined(app: &[PullLink], linked: &[String]) -> Vec<PullLink> {
    let mut all = app.to_vec();
    for link in linked.iter().filter_map(|url| from_url(url)) {
        if !all.iter().any(|l| l.url == link.url) {
            all.push(link);
        }
    }
    all
}

/// The pull request a tile's chip shows, and how many more open ones the session links: the last
/// open one in `links`, or once none is open, the last merged or closed one. A dismissed one never
/// shows.
pub fn shown(links: &[PullLink]) -> Option<(&PullLink, usize)> {
    let mut open = links.iter().filter(|l| !l.dismissed && l.open);
    match open.next_back() {
        Some(newest) => Some((newest, open.count())),
        None => links.iter().rfind(|l| !l.dismissed).map(|l| (l, 0)),
    }
}

/// What `gh pr view` says about a pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pull {
    pub number: u64,
    pub title: String,
    pub state: PullState,
    pub draft: bool,
    pub checks: Checks,
    pub review: Review,
    /// Whether GitHub says it conflicts with its base branch.
    pub conflicts: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PullState {
    Open,
    Merged,
    Closed,
}

/// Where the pull request's checks stand, counting GitHub Actions' check runs and the older
/// commit statuses alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checks {
    None,
    Running,
    Passed,
    /// How many failed, which wins over any still running.
    Failed(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Review {
    None,
    Required,
    Approved,
    ChangesRequested,
}

/// What the chip says at a glance, by its color, from the most pressing fact down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Merged,
    Closed,
    Failed,
    Conflicts,
    Draft,
    Running,
    Passed,
    Open,
}

impl Status {
    /// Whether this status leaves the pull request waiting on someone to fix it, which starts
    /// the tile's timer when its session isn't working on it.
    pub fn broken(self) -> bool {
        matches!(self, Status::Failed | Status::Conflicts)
    }
}

/// The fields `parse` reads, for `gh pr view <url> --json`.
pub const FIELDS: &str = "number,title,state,isDraft,reviewDecision,mergeable,statusCheckRollup";

/// Reads what `gh pr view --json` with `FIELDS` printed (measured with gh 2.91.0,
/// record/areas/rack/log.md, 2026-11-27).
pub fn parse(json: &str) -> Result<Pull, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let text = |key: &str| v.get(key).and_then(Value::as_str).unwrap_or("");
    let state = match text("state") {
        "OPEN" => PullState::Open,
        "MERGED" => PullState::Merged,
        "CLOSED" => PullState::Closed,
        other => return Err(format!("unknown state \"{other}\"")),
    };
    let review = match text("reviewDecision") {
        "APPROVED" => Review::Approved,
        "CHANGES_REQUESTED" => Review::ChangesRequested,
        "REVIEW_REQUIRED" => Review::Required,
        _ => Review::None,
    };
    let rollup = v.get("statusCheckRollup").and_then(Value::as_array);
    Ok(Pull {
        number: v.get("number").and_then(Value::as_u64).unwrap_or(0),
        title: text("title").to_string(),
        state,
        draft: v.get("isDraft").and_then(Value::as_bool) == Some(true),
        checks: checks(rollup.map(Vec::as_slice).unwrap_or(&[])),
        review,
        conflicts: text("mergeable") == "CONFLICTING",
    })
}

// A check run has a `status`, and once it's `COMPLETED` a `conclusion`. A commit status has only a
// `state`. Skipped, neutral, and canceled runs count as neither passed nor failed.
fn checks(rollup: &[Value]) -> Checks {
    let (mut failed, mut running, mut passed) = (0, 0, 0);
    for c in rollup {
        let field = |key: &str| c.get(key).and_then(Value::as_str).unwrap_or("");
        let outcome = match field("__typename") {
            "StatusContext" => field("state"),
            _ if field("status") != "COMPLETED" => "PENDING",
            _ => field("conclusion"),
        };
        match outcome {
            "SUCCESS" => passed += 1,
            "PENDING" | "EXPECTED" => running += 1,
            "FAILURE" | "ERROR" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE" => {
                failed += 1
            }
            _ => {}
        }
    }
    if failed > 0 {
        Checks::Failed(failed)
    } else if running > 0 {
        Checks::Running
    } else if passed > 0 {
        Checks::Passed
    } else {
        Checks::None
    }
}

impl Pull {
    pub fn status(&self) -> Status {
        match (self.state, self.checks) {
            (PullState::Merged, _) => Status::Merged,
            (PullState::Closed, _) => Status::Closed,
            (_, Checks::Failed(_)) => Status::Failed,
            _ if self.conflicts => Status::Conflicts,
            _ if self.draft => Status::Draft,
            (_, Checks::Running) => Status::Running,
            (_, Checks::Passed) => Status::Passed,
            _ => Status::Open,
        }
    }

    /// The pull request's state in words, for the chip's pop-out: "2 checks failed · Changes
    /// requested".
    pub fn words(&self) -> String {
        let mut parts = Vec::new();
        match self.state {
            PullState::Merged => return "Merged".into(),
            PullState::Closed => return "Closed".into(),
            PullState::Open if self.draft => parts.push("Draft".to_string()),
            PullState::Open => {}
        }
        match self.checks {
            Checks::Failed(1) => parts.push("1 check failed".into()),
            Checks::Failed(n) => parts.push(format!("{n} checks failed")),
            Checks::Running => parts.push("Checks running".into()),
            Checks::Passed => parts.push("Checks passed".into()),
            Checks::None => {}
        }
        if self.conflicts {
            parts.push("Merge conflicts".into());
        }
        match self.review {
            Review::Approved => parts.push("Approved".into()),
            Review::ChangesRequested => parts.push("Changes requested".into()),
            Review::Required => parts.push("Review required".into()),
            Review::None => {}
        }
        if parts.is_empty() {
            return "Open".into();
        }
        parts.join(" · ")
    }
}

/// Whether a pull request that went from `was` to `now` has just broken: its checks failed, or
/// it came to conflict with its base. A first look, with no `was`, is no change.
pub fn broke(was: Option<Status>, now: Status) -> bool {
    now.broken() && was.is_some_and(|was| !was.broken())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn link(number: u64, open: bool, dismissed: bool) -> PullLink {
        PullLink {
            url: format!("https://github.com/o/r/pull/{number}"),
            repo: "o/r".into(),
            number,
            open,
            dismissed,
        }
    }

    #[test]
    fn links_read_the_apps_list_and_skip_an_entry_off_github() {
        let prs = json!([
            { "prNumber": 525, "repo": "o/r", "url": "https://github.com/o/r/pull/525",
              "state": "OPEN", "branch": "fix/x" },
            { "prNumber": 7, "repo": "o/r", "url": "https://example.com/o/r/pull/7", "state": "OPEN" },
            { "prNumber": 9, "repo": "o/r", "url": "https://github.com/o/r/pull/9",
              "state": "MERGED", "dismissed": true },
        ]);
        let links = links(Some(&prs));
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].number, 525);
        assert!(links[0].open && !links[0].dismissed);
        assert!(!links[1].open && links[1].dismissed);
        assert!(super::links(None).is_empty());
    }

    #[test]
    fn a_link_reads_a_pull_requests_page_and_leaves_off_what_follows_the_number() {
        let link = from_url("https://github.com/xiprox/plates/pull/8/files?w=1#diff").unwrap();
        assert_eq!(link.url, "https://github.com/xiprox/plates/pull/8");
        assert_eq!(
            (link.repo.as_str(), link.number, link.open),
            ("xiprox/plates", 8, true)
        );
        assert!(from_url("https://github.com/xiprox/plates/issues/8").is_none());
        assert!(from_url("https://github.com/xiprox/plates/pull/eight").is_none());
        assert!(from_url("https://example.com/xiprox/plates/pull/8").is_none());
        assert!(from_url("https://github.com/xiprox/pull/8").is_none());
    }

    #[test]
    fn linked_pull_requests_follow_the_apps_and_skip_one_it_links_too() {
        let app = [link(1, false, false)];
        let linked = [
            "https://github.com/o/r/pull/1".to_string(),
            "https://github.com/o/r/pull/2".to_string(),
            "not a link".to_string(),
        ];
        let all = combined(&app, &linked);
        assert_eq!(all.iter().map(|l| l.number).collect::<Vec<_>>(), [1, 2]);
        assert!(!all[0].open && all[1].open);
    }

    #[test]
    fn the_chip_shows_the_newest_open_pull_request_or_else_the_newest_merged_one() {
        let open = [
            link(1, true, false),
            link(2, false, false),
            link(3, true, false),
        ];
        let (shown_link, more) = shown(&open).unwrap();
        assert_eq!((shown_link.number, more), (3, 1));

        let merged = [
            link(1, false, false),
            link(2, false, false),
            link(3, true, true),
        ];
        let (shown_link, more) = shown(&merged).unwrap();
        assert_eq!((shown_link.number, more), (2, 0));

        assert!(shown(&[link(1, false, true)]).is_none());
        assert!(shown(&[]).is_none());
    }

    fn gh(state: &str, extra: Value) -> Pull {
        let mut v = json!({ "number": 8, "title": "Fix it", "state": state, "isDraft": false,
                            "reviewDecision": "", "mergeable": "MERGEABLE", "statusCheckRollup": [] });
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        parse(&v.to_string()).unwrap()
    }

    fn run(status: &str, conclusion: &str) -> Value {
        json!({ "__typename": "CheckRun", "status": status, "conclusion": conclusion })
    }

    #[test]
    fn a_failed_check_wins_over_running_ones_and_skipped_runs_count_for_nothing() {
        let failing = gh(
            "OPEN",
            json!({ "statusCheckRollup": [run("COMPLETED", "FAILURE"), run("IN_PROGRESS", ""),
                     { "__typename": "StatusContext", "state": "ERROR" }] }),
        );
        assert_eq!(failing.checks, Checks::Failed(2));
        assert_eq!(failing.status(), Status::Failed);
        assert_eq!(failing.words(), "2 checks failed");

        let running = gh(
            "OPEN",
            json!({ "statusCheckRollup": [run("COMPLETED", "SUCCESS"), run("QUEUED", "")] }),
        );
        assert_eq!(running.status(), Status::Running);

        let passed = gh(
            "OPEN",
            json!({ "statusCheckRollup": [run("COMPLETED", "SUCCESS"), run("COMPLETED", "SKIPPED")],
                    "reviewDecision": "APPROVED" }),
        );
        assert_eq!(passed.status(), Status::Passed);
        assert_eq!(passed.words(), "Checks passed · Approved");

        let skipped = gh(
            "OPEN",
            json!({ "statusCheckRollup": [run("COMPLETED", "SKIPPED")] }),
        );
        assert_eq!(skipped.checks, Checks::None);
        assert_eq!(skipped.status(), Status::Open);
        assert_eq!(skipped.words(), "Open");
    }

    #[test]
    fn merged_and_closed_say_only_that_and_a_conflict_or_draft_comes_before_checks() {
        assert_eq!(gh("MERGED", json!({})).status(), Status::Merged);
        assert_eq!(gh("MERGED", json!({})).words(), "Merged");
        assert_eq!(gh("CLOSED", json!({})).status(), Status::Closed);

        let conflicting = gh(
            "OPEN",
            json!({ "mergeable": "CONFLICTING", "statusCheckRollup": [run("COMPLETED", "SUCCESS")] }),
        );
        assert_eq!(conflicting.status(), Status::Conflicts);
        assert_eq!(conflicting.words(), "Checks passed · Merge conflicts");

        let draft = gh(
            "OPEN",
            json!({ "isDraft": true, "reviewDecision": "REVIEW_REQUIRED" }),
        );
        assert_eq!(draft.status(), Status::Draft);
        assert_eq!(draft.words(), "Draft · Review required");

        assert!(parse(r#"{"state": "LOST"}"#).is_err());
    }

    #[test]
    fn only_a_change_into_failed_or_conflicts_breaks_a_pull_request() {
        assert!(broke(Some(Status::Running), Status::Failed));
        assert!(broke(Some(Status::Passed), Status::Conflicts));
        assert!(!broke(None, Status::Failed));
        assert!(!broke(Some(Status::Failed), Status::Conflicts));
        assert!(!broke(Some(Status::Running), Status::Passed));
    }
}
