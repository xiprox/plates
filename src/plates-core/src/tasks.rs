//! A session's task list: the planned work that runs past the request at hand, which Claude keeps
//! through the `plates-tasks` server and the rack shows inside the session's tile, as
//! record/areas/tasks/01-task-list.md designs it. Each list is a JSON file of its own in the
//! folder `tasks::folder` returns, named for the session.

use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::store::ms;

/// The version a list's file is written in, which a later format checks before reading.
const VERSION: u32 = 1;

/// Where a task stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Todo,
    Doing,
    Waiting,
    Done,
    Dropped,
}

impl State {
    /// The names Claude passes, in the order the server's tools list them.
    pub const NAMES: [&str; 5] = ["todo", "doing", "waiting", "done", "dropped"];

    pub fn parse(name: &str) -> Result<State, Error> {
        match name {
            "todo" => Ok(State::Todo),
            "doing" => Ok(State::Doing),
            "waiting" => Ok(State::Waiting),
            "done" => Ok(State::Done),
            "dropped" => Ok(State::Dropped),
            _ => Err(Error::BadState(name.to_string())),
        }
    }
}

/// One piece of planned work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: u32,
    pub title: String,
    pub state: State,
    /// The latest on the task in one line, which each update replaces.
    #[serde(default)]
    pub status: String,
    /// Who or what a waiting task waits on: "you" for the user, or a name.
    #[serde(default)]
    pub waiting_on: String,
    /// When the task last changed, in milliseconds since 1970.
    pub updated: u64,
}

impl Task {
    pub fn holder(&self) -> Holder {
        match self.state {
            State::Waiting if self.waits_on_you() => Holder::You,
            State::Waiting => Holder::Others,
            State::Doing => Holder::Doing,
            State::Todo => Holder::Todo,
            State::Done => Holder::Done,
            State::Dropped => Holder::Dropped,
        }
    }

    /// Whether the task waits on the user, which the rack points out.
    /// The server's instructions ask Claude for "you", and "user" and "the user" mean the same.
    pub fn waits_on_you(&self) -> bool {
        let on = self.waiting_on.trim();
        self.state == State::Waiting
            && ["you", "user", "the user"]
                .iter()
                .any(|name| on.eq_ignore_ascii_case(name))
    }
}

/// A task's fields as Claude gives them, each left out to keep what the task has.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub title: Option<String>,
    pub state: Option<State>,
    pub status: Option<String>,
    pub waiting_on: Option<String>,
}

impl Change {
    /// Checks the fields given against their limits, and that a new task has a title.
    fn check(&self, new: bool) -> Result<(), Error> {
        let title = self.title.as_deref().map(str::trim);
        if title.is_some_and(str::is_empty) || (new && title.is_none()) {
            return Err(Error::EmptyTitle);
        }
        for (text, field) in [
            (title, Field::Title),
            (self.status.as_deref().map(str::trim), Field::Status),
            (self.waiting_on.as_deref().map(str::trim), Field::WaitingOn),
        ] {
            if let Some(text) = text {
                check_field(text, field)?;
            }
        }
        Ok(())
    }
}

/// The list `tasks_replace` replaced, kept so a list replaced by mistake can come back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replaced {
    pub at: u64,
    pub tasks: Vec<Task>,
}

/// A session's list, as its file holds it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskList {
    pub version: u32,
    /// Whether the user said not to keep a list in this session, which refuses new tasks.
    #[serde(default)]
    pub declined: bool,
    pub next_id: u32,
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub replaced: Option<Replaced>,
}

impl Default for TaskList {
    fn default() -> Self {
        TaskList {
            version: VERSION,
            declined: false,
            next_id: 1,
            tasks: Vec::new(),
            replaced: None,
        }
    }
}

/// Why the server refused a call, worded for Claude to read.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    BadState(String),
    EmptyTitle,
    TooLong(Field),
    NotOneLine(Field),
    /// A tool's text that has to be given, such as `wait`'s `on`, was left out or empty.
    Missing(Field),
    NoTask(u32),
    Declined,
}

/// Text with a length limit: a task's fields, and the text `wait` and `ping` take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Title,
    Status,
    WaitingOn,
    /// What `wait` waits on, which the tile's summary line shows after "Waiting on ", so it takes
    /// `waiting_on`'s limit.
    On,
    /// A ping's message, which the tile shows as its summary line, so it takes a status's limit.
    Message,
}

impl Field {
    /// The most characters the field takes. The tile shows a title and a status line in one
    /// line each, at the rack's width, where about 30 and 40 characters fit, and a task waiting
    /// on someone shows who after its status (record/areas/tasks/01-task-list.md). When you change
    /// one, check the tile's rows in `plates showcase --tasks`.
    pub const fn max(self) -> usize {
        match self {
            Field::Title => 30,
            Field::Status => 40,
            Field::WaitingOn => 20,
            Field::On => Field::WaitingOn.max(),
            Field::Message => Field::Status.max(),
        }
    }
}

/// Checks one line of text a tool takes on its own, such as `wait`'s `on`, and returns it
/// trimmed. The server checks a call's text with it, and so does Plates, which learns of the call
/// from `PreToolUse` before the server answers, so both keep the same calls
/// (record/areas/tasks/02-waits-and-pings.md).
pub fn check_line(text: &str, field: Field) -> Result<String, Error> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Error::Missing(field));
    }
    check_field(text, field)?;
    Ok(text.to_string())
}

// Checks text against its field's limit, and that it's one line, since the tile shows each field
// on one line, and a line break would draw a second line over the row below.
fn check_field(text: &str, field: Field) -> Result<(), Error> {
    if text.chars().count() > field.max() {
        return Err(Error::TooLong(field));
    }
    if text.chars().any(char::is_control) {
        return Err(Error::NotOneLine(field));
    }
    Ok(())
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::BadState(name) => write!(
                f,
                "\"{name}\" isn't a state. Use one of: {}.",
                State::NAMES.join(", ")
            ),
            Error::EmptyTitle => write!(f, "A task needs a title."),
            Error::TooLong(Field::Title) => write!(
                f,
                "A title shows in one line of about {} characters. Name the piece of work in a few \
                 words, such as \"Windows probe\".",
                Field::Title.max()
            ),
            Error::TooLong(Field::Status) => write!(
                f,
                "A status shows in one line of about {} characters. Say what's new about the task \
                 in a few words, such as \"Needs a session on the PC\", without repeating its \
                 title.",
                Field::Status.max()
            ),
            Error::TooLong(Field::WaitingOn) => write!(
                f,
                "waiting_on takes up to {} characters. Name who or what in a word or two, such \
                 as \"Dana\" or \"the data team\".",
                Field::WaitingOn.max()
            ),
            Error::TooLong(Field::On) => write!(
                f,
                "on takes up to {} characters. Name what you're waiting on in a word or two, such \
                 as \"CI checks\".",
                Field::On.max()
            ),
            Error::TooLong(Field::Message) => write!(
                f,
                "A ping's message shows in one line of about {} characters. Say what happened in a \
                 few words, such as \"Checks passed; ready to merge #12\".",
                Field::Message.max()
            ),
            Error::Missing(Field::On) => write!(
                f,
                "wait needs on: what you're waiting on, such as \"CI checks\"."
            ),
            Error::Missing(Field::Message) => write!(
                f,
                "ping needs a message: what happened, such as \"Checks passed; ready to merge #12\"."
            ),
            Error::Missing(field) => write!(f, "{field:?} needs text."),
            Error::NotOneLine(field) => {
                let name = match field {
                    Field::Title => "A title",
                    Field::Status => "A status",
                    Field::WaitingOn => "waiting_on",
                    Field::On => "on",
                    Field::Message => "A ping's message",
                };
                write!(
                    f,
                    "{name} shows on one line. Write it without line breaks or tabs."
                )
            }
            Error::NoTask(id) => write!(f, "No task has id {id}. Call tasks_get to see the list."),
            Error::Declined => write!(
                f,
                "The user said not to keep a task list in this session, so it takes no tasks."
            ),
        }
    }
}

/// Who holds a task, which the rack groups a list by, since a session's pieces of work often
/// move at once, and what you want back when you return to one is who has each piece
/// (record/areas/tasks/01-task-list.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Holder {
    /// Under way.
    Doing,
    /// Waiting on you.
    You,
    /// Waiting on someone or something else.
    Others,
    /// Not started, and waiting on no one.
    Todo,
    Done,
    Dropped,
}

impl Holder {
    /// The groups open work falls into, in the order the rack shows them. Doing comes first,
    /// since it's what changes from turn to turn, while what's on you changes only when you act
    /// and can pile up above it (user, 2026-11-21).
    pub const OPEN: [Holder; 4] = [Holder::Doing, Holder::You, Holder::Others, Holder::Todo];
}

/// How many of a list's tasks each holder has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Holders {
    pub you: usize,
    pub doing: usize,
    pub others: usize,
    pub todo: usize,
    pub done: usize,
    pub dropped: usize,
}

impl Holders {
    pub fn of(&self, holder: Holder) -> usize {
        match holder {
            Holder::You => self.you,
            Holder::Doing => self.doing,
            Holder::Others => self.others,
            Holder::Todo => self.todo,
            Holder::Done => self.done,
            Holder::Dropped => self.dropped,
        }
    }
}

impl TaskList {
    /// Adds a task, as `todo` unless `change` gives a state.
    pub fn add(&mut self, change: Change, now: SystemTime) -> Result<&Task, Error> {
        if self.declined {
            return Err(Error::Declined);
        }
        let task = self.new_task(change, now)?;
        self.tasks.push(task);
        Ok(self.tasks.last().expect("the task was just pushed"))
    }

    /// Changes the fields `change` gives, and keeps the rest.
    pub fn update(&mut self, id: u32, change: Change, now: SystemTime) -> Result<&Task, Error> {
        change.check(false)?;
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or(Error::NoTask(id))?;
        if let Some(title) = change.title {
            task.title = title.trim().to_string();
        }
        if let Some(state) = change.state {
            task.state = state;
        }
        if let Some(status) = change.status {
            task.status = status.trim().to_string();
        }
        if let Some(waiting_on) = change.waiting_on {
            task.waiting_on = waiting_on.trim().to_string();
        }
        task.updated = ms(now);
        Ok(task)
    }

    /// Replaces every task, keeping the list it replaced. An empty `tasks` wipes the list.
    pub fn replace(&mut self, tasks: Vec<Change>, now: SystemTime) -> Result<(), Error> {
        if self.declined {
            return Err(Error::Declined);
        }
        // Every task is checked before any changes, so a bad one leaves the list as it was.
        for change in &tasks {
            change.check(true)?;
        }
        let old = std::mem::take(&mut self.tasks);
        for change in tasks {
            let task = self.new_task(change, now)?;
            self.tasks.push(task);
        }
        // Replacing an empty list, as after a wipe, keeps the list the wipe replaced, so it can
        // still come back.
        if !old.is_empty() {
            self.replaced = Some(Replaced {
                at: ms(now),
                tasks: old,
            });
        }
        Ok(())
    }

    /// Records that the user doesn't want a list in this session.
    pub fn decline(&mut self) {
        self.declined = true;
    }

    /// How many tasks each holder has, or none for a list without tasks, which the rack shows
    /// no bar for.
    pub fn holders(&self) -> Option<Holders> {
        if self.tasks.is_empty() {
            return None;
        }
        let mut h = Holders::default();
        for t in &self.tasks {
            *match t.holder() {
                Holder::You => &mut h.you,
                Holder::Doing => &mut h.doing,
                Holder::Others => &mut h.others,
                Holder::Todo => &mut h.todo,
                Holder::Done => &mut h.done,
                Holder::Dropped => &mut h.dropped,
            } += 1;
        }
        Some(h)
    }

    fn new_task(&mut self, change: Change, now: SystemTime) -> Result<Task, Error> {
        change.check(true)?;
        let title = change.title.unwrap_or_default().trim().to_string();
        let task = Task {
            id: self.next_id,
            title,
            state: change.state.unwrap_or(State::Todo),
            status: change.status.unwrap_or_default().trim().to_string(),
            waiting_on: change.waiting_on.unwrap_or_default().trim().to_string(),
            updated: ms(now),
        };
        self.next_id += 1;
        Ok(task)
    }
}

/// The folder of every session's list, which the task server writes and the rack reads:
/// `%USERPROFILE%\.plates\tasks\` on Windows, and `tasks` in Plates' data folder on macOS.
///
/// On Windows, the folder is outside `%APPDATA%` so that a list reaches the rack when the server
/// runs inside the Claude desktop app's package, as a session's commands do. When a program in
/// the package writes a new file under `%APPDATA%`, or renames one over an existing file, Windows
/// puts the result in the package's own folder, where the rack never sees it. The same writes to
/// the home folder go through (measured on Windows 11 25H2, record/areas/tasks/log.md).
pub fn folder() -> PathBuf {
    match dirs::home_dir() {
        Some(home) if cfg!(windows) => home.join(".plates").join("tasks"),
        _ => crate::store::data_dir().join("tasks"),
    }
}

/// The file of `session`'s list in `folder`, or none for an ID that isn't safe as a file name.
/// The Claude desktop app's IDs, such as `local_<uuid>`, and Claude Code's UUIDs are.
pub fn path(folder: &Path, session: &str) -> Option<PathBuf> {
    let safe = !session.is_empty()
        && session
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    safe.then(|| folder.join(format!("{session}.json")))
}

/// Reads the list at `path`. A missing file is an empty list.
pub fn load(path: &Path) -> io::Result<TaskList> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(TaskList::default()),
        Err(e) => Err(e),
    }
}

/// Writes the list to `path`, so Plates, which reads the folder once a second, never reads half a
/// list.
pub fn save(list: &TaskList, path: &Path) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(list).map_err(io::Error::other)?;
    crate::store::write_whole(path, &json)
}

/// Deletes `session`'s list from `folder`, once its tile has gone for good
/// (record/areas/tasks/01-task-list.md, "Saving"). A list that isn't there is already gone.
pub fn delete(folder: &Path, session: &str) -> io::Result<()> {
    let Some(path) = path(folder, session) else {
        return Ok(());
    };
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Every session's list in the tasks folder, for the rack, which shows each on its session's tile.
/// The rack reads the folder once a second, so a read rereads only the files that changed.
pub struct Watch {
    folder: PathBuf,
    /// Each file's modified time and length at its last read.
    stamps: HashMap<PathBuf, (SystemTime, u64)>,
    lists: HashMap<String, TaskList>,
}

impl Watch {
    pub fn new(folder: PathBuf) -> Self {
        Watch {
            folder,
            stamps: HashMap::new(),
            lists: HashMap::new(),
        }
    }

    /// Rereads the files that changed, and returns every list by its session's ID when any list
    /// was added, changed, or deleted since the last read.
    pub fn poll(&mut self) -> Option<HashMap<String, TaskList>> {
        let mut seen = HashSet::new();
        let mut changed = false;
        // A missing folder holds no lists, as before any session kept one.
        for entry in fs::read_dir(&self.folder).into_iter().flatten().flatten() {
            let path = entry.path();
            // The server's half-written files end in `.new`, so they're left out here.
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(session) = path.file_stem().and_then(|s| s.to_str()).map(String::from) else {
                continue;
            };
            let Ok(meta) = entry.metadata() else { continue };
            let stamp = (meta.modified().unwrap_or(UNIX_EPOCH), meta.len());
            seen.insert(session.clone());
            if self.stamps.get(&path) == Some(&stamp) {
                continue;
            }
            // A file that doesn't parse keeps the list from its last good read.
            if let Ok(list) = load(&path) {
                self.stamps.insert(path, stamp);
                self.lists.insert(session, list);
                changed = true;
            }
        }
        let before = self.lists.len();
        self.lists.retain(|session, _| seen.contains(session));
        self.stamps.retain(|path, _| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| seen.contains(s))
        });
        changed |= self.lists.len() != before;
        changed.then(|| self.lists.clone())
    }
}

/// A time from milliseconds since 1970 as UTC in ISO 8601, such as "2026-11-20T11:42:11Z", for
/// Claude to read.
pub fn iso(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rest) = (secs / 86_400, secs % 86_400);
    let (h, m, s) = (rest / 3600, rest % 3600 / 60, rest % 60);
    // Days since 1970 to a date in the proleptic Gregorian calendar, by Howard Hinnant's
    // `civil_from_days`, which counts in 400-year eras that start on March 1.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(ms: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(ms)
    }

    fn titled(title: &str) -> Change {
        Change {
            title: Some(title.to_string()),
            ..Change::default()
        }
    }

    #[test]
    fn adding_numbers_tasks_and_starts_them_as_todo() {
        let mut list = TaskList::default();
        list.add(titled("Write the design"), at(1)).unwrap();
        let second = list.add(titled("Build the server"), at(2)).unwrap();
        assert_eq!(second.id, 2);
        assert_eq!(second.state, State::Todo);
        assert_eq!(second.updated, 2);
    }

    #[test]
    fn a_task_needs_a_title() {
        let mut list = TaskList::default();
        assert_eq!(list.add(titled("  "), at(1)), Err(Error::EmptyTitle));
        assert!(list.tasks.is_empty());
    }

    #[test]
    fn updating_changes_only_the_fields_given() {
        let mut list = TaskList::default();
        list.add(
            Change {
                status: Some("Drafted".into()),
                ..titled("Write the design")
            },
            at(1),
        )
        .unwrap();
        let change = Change {
            state: Some(State::Waiting),
            waiting_on: Some("you".into()),
            ..Change::default()
        };
        let task = list.update(1, change, at(5)).unwrap().clone();
        assert_eq!(task.title, "Write the design");
        assert_eq!(task.status, "Drafted");
        assert!(task.waits_on_you());
        assert_eq!(task.updated, 5);
        assert_eq!(
            list.update(9, Change::default(), at(6)),
            Err(Error::NoTask(9))
        );
    }

    #[test]
    fn replacing_keeps_the_old_list_and_an_empty_one_wipes() {
        let mut list = TaskList::default();
        list.add(titled("Old"), at(1)).unwrap();
        list.replace(vec![titled("New")], at(2)).unwrap();
        assert_eq!(list.tasks[0].title, "New");
        assert_eq!(list.tasks[0].id, 2);
        assert_eq!(list.replaced.as_ref().unwrap().tasks[0].title, "Old");
        list.replace(Vec::new(), at(3)).unwrap();
        assert!(list.tasks.is_empty());
        assert_eq!(list.replaced.as_ref().unwrap().tasks[0].title, "New");
    }

    #[test]
    fn titles_statuses_and_names_fit_one_line() {
        let mut list = TaskList::default();
        let long = "x".repeat(Field::Title.max() + 1);
        assert_eq!(
            list.add(titled(&long), at(1)),
            Err(Error::TooLong(Field::Title))
        );
        assert!(list.tasks.is_empty());
        list.add(titled(&"x".repeat(Field::Title.max())), at(1))
            .unwrap();
        let change = Change {
            status: Some("y".repeat(Field::Status.max() + 1)),
            ..Change::default()
        };
        assert_eq!(
            list.update(1, change, at(2)),
            Err(Error::TooLong(Field::Status))
        );
        assert_eq!(list.tasks[0].status, "", "a refused update changes nothing");
        let bad = vec![Change {
            waiting_on: Some("z".repeat(Field::WaitingOn.max() + 1)),
            ..titled("ok")
        }];
        assert_eq!(
            list.replace(bad, at(3)),
            Err(Error::TooLong(Field::WaitingOn))
        );
    }

    #[test]
    fn a_line_break_or_tab_is_refused_and_the_user_has_three_names() {
        let mut list = TaskList::default();
        assert_eq!(
            list.add(titled("Two\nlines"), at(1)),
            Err(Error::NotOneLine(Field::Title))
        );
        for name in ["you", "User", "the user"] {
            let change = Change {
                state: Some(State::Waiting),
                waiting_on: Some(name.into()),
                ..titled("Pick a style")
            };
            assert!(list.add(change, at(1)).unwrap().waits_on_you(), "{name}");
        }
    }

    #[test]
    fn a_list_wiped_and_then_filled_keeps_the_list_the_wipe_replaced() {
        let mut list = TaskList::default();
        list.add(titled("Real"), at(1)).unwrap();
        list.replace(Vec::new(), at(2)).unwrap();
        list.replace(vec![titled("New")], at(3)).unwrap();
        assert_eq!(list.replaced.as_ref().unwrap().tasks[0].title, "Real");
    }

    #[test]
    fn a_replacement_with_a_bad_task_leaves_the_list_alone() {
        let mut list = TaskList::default();
        list.add(titled("Keep"), at(1)).unwrap();
        let bad = vec![titled("Fine"), Change::default()];
        assert_eq!(list.replace(bad, at(2)), Err(Error::EmptyTitle));
        assert_eq!(list.tasks[0].title, "Keep");
        assert!(list.replaced.is_none());
    }

    #[test]
    fn a_declined_list_takes_no_tasks_but_keeps_its_own() {
        let mut list = TaskList::default();
        list.add(titled("Keep"), at(1)).unwrap();
        list.decline();
        assert_eq!(list.add(titled("More"), at(2)), Err(Error::Declined));
        assert_eq!(list.replace(Vec::new(), at(2)), Err(Error::Declined));
        assert!(list.update(1, Change::default(), at(3)).is_ok());
        assert_eq!(list.tasks.len(), 1);
    }

    #[test]
    fn each_task_has_one_holder_and_the_list_counts_them() {
        let mut list = TaskList::default();
        assert_eq!(list.holders(), None);
        for (title, state, on) in [
            ("a", State::Done, ""),
            ("b", State::Dropped, ""),
            ("c", State::Waiting, "Dana"),
            ("d", State::Waiting, "You"),
            ("e", State::Doing, ""),
            ("f", State::Todo, ""),
            ("g", State::Todo, ""),
        ] {
            let change = Change {
                state: Some(state),
                waiting_on: Some(on.into()),
                ..titled(title)
            };
            list.add(change, at(1)).unwrap();
        }
        let h = list.holders().unwrap();
        assert_eq!((h.you, h.doing, h.others, h.todo), (1, 1, 1, 2));
        assert_eq!((h.done, h.dropped), (1, 1));
        assert_eq!(
            list.tasks[3].holder(),
            Holder::You,
            "\"You\" in any case is you"
        );
    }

    #[test]
    fn a_saved_list_loads_back_the_same_and_a_missing_one_is_empty() {
        let dir = std::env::temp_dir().join(format!("plates-core-tasks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = path(&dir, "local_0f1e2d3c-aaaa-bbbb-cccc-0123456789ab").unwrap();
        assert_eq!(load(&path).unwrap(), TaskList::default());
        let mut list = TaskList::default();
        list.add(titled("Write the design"), at(1)).unwrap();
        list.decline();
        save(&list, &path).unwrap();
        assert_eq!(load(&path).unwrap(), list);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_watch_reports_a_list_added_changed_and_deleted_once_each() {
        let dir = std::env::temp_dir().join(format!("plates-core-watch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut watch = Watch::new(dir.clone());
        assert_eq!(watch.poll(), None, "a missing folder holds no lists");
        let file = path(&dir, "local_a").unwrap();
        let mut list = TaskList::default();
        list.add(titled("One"), at(1)).unwrap();
        save(&list, &file).unwrap();
        assert_eq!(watch.poll().unwrap()["local_a"].tasks.len(), 1);
        assert_eq!(watch.poll(), None, "an unchanged file reports nothing");
        list.add(titled("Two, a longer title"), at(2)).unwrap();
        save(&list, &file).unwrap();
        assert_eq!(watch.poll().unwrap()["local_a"].tasks.len(), 2);
        fs::remove_file(&file).unwrap();
        assert!(watch.poll().unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_session_id_that_isnt_a_safe_file_name_has_no_path() {
        let dir = Path::new("tasks");
        assert!(path(dir, "local_0f1e2d3c").is_some());
        assert!(path(dir, "../rack").is_none());
        assert!(path(dir, "").is_none());
    }

    #[test]
    fn iso_writes_utc_dates() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        // Python's datetime gives 2026-11-20T11:42:11 UTC for 1,795,174,931s.
        assert_eq!(iso(1_795_174_931_000), "2026-11-20T11:42:11Z");
        // A leap day.
        assert_eq!(iso(951_782_400_000), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn deleting_a_list_removes_its_file_and_a_missing_one_is_already_gone() {
        let dir = std::env::temp_dir().join(format!("plates-tasks-delete-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = path(&dir, "local_a").unwrap();
        save(&TaskList::default(), &file).unwrap();
        delete(&dir, "local_a").unwrap();
        assert!(!file.exists());
        delete(&dir, "local_a").unwrap();
        delete(&dir, "../escape").unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }
}
