use chrono::Duration;
use console::{Style, StyledObject};
use google_tasks1::api::Task as GTask;
use indicatif::ProgressIterator;
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io};

use crate::auth::Authenticator;
use crate::cfg::CFG;
use crate::opts::Sync as SyncOpts;
use crate::parse::{ParsedNorg, State, Todo};
use crate::progress_bar::style_progress_bar_count;
use crate::tasks::{clear_tasks, get_tasks, task_complete, task_create, task_update, Task};
use crate::Error;

pub async fn perform_sync(auth: Authenticator, opts: &SyncOpts) -> Result<(), Error> {
    let tasklist = CFG.tasklist.clone();
    let files = {
        let mut files =
            get_files_from_folders(&opts.files_or_folders[..], &CFG.ignore_filenames[..])?;
        if !opts.without_sort {
            files.sort();
        }
        files
    };

    let mut todos = Vec::new();
    let mut tasks = get_tasks(auth.clone(), &tasklist).await?;
    let original_tasks = tasks.clone();

    let mut stats = Vec::new();

    let idx_last = files.len() - 1;
    for (i, file) in files
        .iter()
        .enumerate()
        .progress_with_style(style_progress_bar_count())
        .with_message("Syncing…")
    {
        // Skip the file we want to pull to
        match (i, opts.pull_to_first) {
            (0, true) => continue,
            (idx, false) if idx == idx_last => continue,
            _ => {}
        }

        let mut syncer = Syncer::from_opts(opts, tasklist.clone());
        syncer.pull_new = false;

        let result = syncer.perform(auth.clone(), file, &tasks[..]).await?;
        tasks = result.tasks_after;
        todos.extend(result.todos_present);

        stats.push(result.stats);
    }

    // Sync file that we pull to
    let present_todo_ids: Vec<Arc<str>> = todos.iter().filter_map(|t| t.id.clone()).collect();
    // tasks that were actually created new
    let new_remote_tasks = original_tasks
        .iter()
        .filter(|t| !present_todo_ids.contains(&t.id))
        .cloned()
        .collect::<Vec<_>>();

    let file_to_pull = &files[if opts.pull_to_first { 0 } else { idx_last }];

    let result = Syncer::from_opts(opts, tasklist.clone())
        .perform(auth.clone(), file_to_pull, &new_remote_tasks[..])
        .await?;
    if opts.pull_to_first {
        stats.insert(0, result.stats);
    } else {
        stats.push(result.stats);
    }

    let num_deleted = if let Some(days) = CFG.clear_completed_tasks_older_than_days {
        let (tasks, num_deleted) =
            clear_tasks(auth, &tasklist, tasks, Duration::days(days as i64)).await?;
        log::info!(
            "Number of tasks not completed/old enough yet: {}",
            tasks.len()
        );
        num_deleted
    } else {
        log::info!("Not clearing old completed tasks.");
        0
    };

    for s in stats.iter().filter(|s| s.any_change()) {
        println!("{}", s);
    }

    if num_deleted > 0 {
        println!(
            "Cleared {} completed tasks older than {} days…",
            num_deleted,
            CFG.clear_completed_tasks_older_than_days.unwrap()
        );
    }

    Ok(())
}

fn get_files_from_folders<P, S>(
    files_or_folders: &[P],
    ignored_filenames: &[S],
) -> Result<Vec<PathBuf>, Error>
where
    P: AsRef<Path>,
    S: AsRef<str>,
{
    let mut files = Vec::new();
    let ignored_filenames: Vec<&str> = ignored_filenames.iter().map(|s| s.as_ref()).collect();

    for p in files_or_folders.iter().map(|p| p.as_ref()) {
        if p.is_dir() {
            let paths = fs::read_dir(p)?.collect::<io::Result<Vec<_>>>()?;

            for entry in paths {
                let p = entry.path();
                let file_name = p
                    .file_name()
                    .map(|f| f.to_string_lossy())
                    .unwrap_or(std::borrow::Cow::Owned(String::new()));
                if p.is_file()
                    && p.extension() == Some(&OsString::from("norg"))
                    && !ignored_filenames.contains(&file_name.as_ref())
                {
                    files.push(p);
                }
            }
        } else if p.is_file() {
            files.push(p.to_owned());
        }
    }
    Ok(files)
}

struct Syncer {
    fix_missing: bool,

    pull_completed: bool,
    push_completed: bool,
    pull_new: bool,
    push_new: bool,

    tasklist: Arc<str>,
}

#[derive(Debug, Clone)]
struct SyncStats {
    file: PathBuf,
    num_pull_completed: usize,
    num_push_completed: usize,
    num_pull_new: usize,
    num_push_new: usize,
    num_newer_local: usize,
    num_newer_remote: usize,
}

#[derive(Debug, Clone)]
struct Diff {
    newer_local: HashMap<Arc<str>, Todo>,
    newer_remote: HashMap<Arc<str>, Task>,
}

impl Diff {
    fn compute(local: &ParsedNorg, remote: &[Task]) -> Result<Diff, Error> {
        let todos: HashMap<Arc<str>, &Todo> = local
            .todos
            .iter()
            .filter_map(|t| t.id.clone().map(|id| (id, t)))
            .collect();
        let tasks: HashMap<Arc<str>, &Task> = remote.iter().map(|t| (t.id.clone(), t)).collect();

        let mut newer_local = HashMap::new();
        let mut newer_remote = HashMap::new();

        for (id, todo) in todos {
            let task = match (tasks.get(&id), todo.state) {
                (Some(task), _) => task,
                (None, State::Done) => {
                    // it's okay if completed remote task are missing
                    continue;
                }
                (None, _) => {
                    return Err(Error::NotFound {
                        what: format!("remote task for todo '{}'", todo.content),
                    });
                }
            };

            let title_differs = task.title.trim() != todo.content.trim();
            let local_newer = task.modified_at < local.modified_at;
            // for now we only sync differing due date to remote
            if title_differs {
                if local_newer {
                    newer_local.insert(id, todo.clone());
                } else {
                    newer_remote.insert(id, (**task).clone());
                }
                continue;
            }

            let due_date_differs = task.due_at != todo.due_at;

            if due_date_differs && local_newer {
                newer_local.insert(id, todo.clone());
                continue;
            }
        }
        Ok(Diff {
            newer_local,
            newer_remote,
        })
    }
}

static DONE: Lazy<StyledObject<&str>> = Lazy::new(|| STYLE_DONE.apply_to("✓").bold());
static NEW: Lazy<StyledObject<&str>> = Lazy::new(|| STYLE_NEW.apply_to("✻").bold());
static PULL: Lazy<StyledObject<&str>> = Lazy::new(|| STYLE_PULL.apply_to("↘").bold());
static PUSH: Lazy<StyledObject<&str>> = Lazy::new(|| STYLE_PUSH.apply_to("↗").bold());
static UPDATE: Lazy<StyledObject<&str>> = Lazy::new(|| STYLE_UPDATE.apply_to("⟳").bold());

static STYLE_DONE: Lazy<Style> = Lazy::new(|| Style::new().green());
static STYLE_NEW: Lazy<Style> = Lazy::new(|| Style::new().cyan());
static STYLE_PULL: Lazy<Style> = Lazy::new(|| Style::new().blue());
static STYLE_PUSH: Lazy<Style> = Lazy::new(|| Style::new().magenta());
static STYLE_UPDATE: Lazy<Style> = Lazy::new(|| Style::new().yellow());

impl std::fmt::Display for SyncStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let completed = DONE.to_string();
        let new = NEW.to_string();
        let pulled = PULL.to_string();
        let pushed = PUSH.to_string();
        let updated = UPDATE.to_string();

        write!(f, "{file}: {completed} {pulled} {pull_completed} {pushed} {push_completed} | {new} {pulled} {pull_new} {pushed} {push_new} | {updated} {pulled} {newer_remote} {pushed} {newer_local}",
        file=self.file.display(),
        pull_completed=STYLE_DONE.apply_to(self.num_pull_completed),
        push_completed=STYLE_DONE.apply_to(self.num_push_completed),
        pull_new=STYLE_NEW.apply_to(self.num_pull_new),
        push_new=STYLE_NEW.apply_to(self.num_push_new),
        newer_local=STYLE_UPDATE.apply_to(self.num_newer_local),
        newer_remote=STYLE_UPDATE.apply_to(self.num_newer_remote),
        )
    }
}

impl SyncStats {
    fn any_change(&self) -> bool {
        (self.num_pull_new
            + self.num_pull_completed
            + self.num_push_new
            + self.num_push_completed
            + self.num_newer_local
            + self.num_newer_remote)
            > 0
    }
    fn modified_file(&self) -> bool {
        (self.num_pull_new + self.num_pull_completed + self.num_push_new + self.num_newer_remote)
            > 0
    }
}

#[derive(Debug, Clone)]
struct SyncResult {
    tasks_after: Vec<Task>,
    todos_present: Vec<Todo>,
    stats: SyncStats,
}

impl Syncer {
    // Perform full sync, returning newly created tasks
    async fn perform(
        &self,
        auth: Authenticator,
        file: &Path,
        tasks: &[Task],
    ) -> Result<SyncResult, Error> {
        let mut norg = ParsedNorg::open(file)?;

        let mut tasks_after: Vec<Task> = tasks.to_vec();

        let mut num_pull_completed = 0;
        let mut num_push_completed = 0;
        let mut num_pull_new = 0;
        let mut num_push_new = 0;

        log::trace!("Pre-pull completed:\n{norg:#?}");
        if self.pull_completed {
            num_pull_completed = sync_pull_completed(tasks, &mut norg)?;
        }
        log::trace!("Pre-pull new:\n{norg:#?}");
        if self.pull_new {
            num_pull_new = sync_pull_new(tasks, &mut norg)?;
        }

        log::trace!("Pre-push completed:\n{norg:#?}");
        if self.push_completed {
            num_push_completed =
                sync_push_completed(auth.clone(), &self.tasklist, &mut norg, tasks).await?;
        }

        let missing = check_missing_remote_tasks(&tasks_after[..], &norg);
        if self.fix_missing {
            let missing_idx = norg
                .todos
                .iter()
                .enumerate()
                .filter_map(|(idx, t)| {
                    if missing.iter().any(|m| m.id == t.id) {
                        Some(idx)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let num = missing_idx.len();
            log::info!("Clearing {num} tasks that are not present remote to re-create them.");
            norg.clear_tags(&missing_idx)?;
        } else {
            warn_missing_remote_tasks(file, missing);
        }

        log::trace!("Pre-push new:\n{norg:#?}");
        if self.push_new {
            let pushed = sync_push_new(auth.clone(), &self.tasklist, &mut norg).await?;
            num_push_new = pushed.len();
            tasks_after.extend(pushed);
        }

        let diff = Diff::compute(&norg, &tasks_after[..])?;

        let stats = SyncStats {
            file: file.to_path_buf(),
            num_pull_completed,
            num_push_completed,
            num_pull_new,
            num_push_new,
            num_newer_local: diff.newer_local.len(),
            num_newer_remote: diff.newer_remote.len(),
        };

        for (id, todo) in diff.newer_local {
            let new_gtask: GTask = task_update(auth.clone(), &self.tasklist, &todo).await?;
            let task = Task::try_from(&new_gtask)?;
            let idx = idx_by_task_id(&tasks_after[..], &id);
            tasks_after[idx] = task;
        }

        let updates: Vec<_> = diff
            .newer_remote
            .into_iter()
            .map(|(id, t)| (norg.idx_by_todo_id(&id), t.title.clone()))
            .collect();

        norg.update_task_titles(updates)?;

        if stats.modified_file() {
            norg.backup()?;
            norg.write()?;
        }
        Ok(SyncResult {
            tasks_after,
            todos_present: norg.todos,
            stats,
        })
    }

    fn from_opts(opts: &SyncOpts, tasklist: Arc<str>) -> Syncer {
        Syncer {
            fix_missing: opts.fix_missing,

            pull_completed: !opts.without_local,
            push_completed: !opts.without_remote,
            pull_new: !opts.without_local && !opts.without_pull,
            push_new: !opts.without_remote && !opts.without_push,

            tasklist,
        }
    }
}

pub fn idx_by_task_id(tasks: &[Task], id: &str) -> usize {
    tasks
        .iter()
        .enumerate()
        .find_map(|(idx, t)| if t.id.as_ref() == id { Some(idx) } else { None })
        .unwrap()
}

// Sync completed tasks from remote to neorg
fn sync_pull_completed(tasks: &[Task], norg: &mut ParsedNorg) -> Result<usize, Error> {
    let remote_done: HashSet<Arc<str>> = tasks
        .iter()
        .filter_map(|t| {
            if t.completed {
                Some(t.id.clone())
            } else {
                None
            }
        })
        .collect();

    let idx_to_complete: Vec<_> = norg
        .todos
        .iter_mut()
        .enumerate()
        .filter_map(|(i, t)| {
            if t.state != State::Done
                && t.id.is_some()
                && remote_done.contains(&t.id.clone().unwrap())
            {
                t.state = State::Done;
                Some(i)
            } else {
                None
            }
        })
        .collect();

    let mut count = 0;
    for idx in idx_to_complete {
        norg.mark_completed(idx);
        count += 1;
    }
    Ok(count)
}

// Sync completed tasks from neorg to remote, return how many were synced.
async fn sync_push_completed(
    auth: Authenticator,
    tasklist: &str,
    norg: &mut ParsedNorg,
    tasks: &[Task],
) -> Result<usize, Error> {
    let norg_done: HashSet<Arc<str>> = norg
        .todos
        .iter()
        .filter_map(|t| match (t.id.as_ref(), &t.state) {
            (Some(id), State::Done) => Some(id.clone()),
            _ => None,
        })
        .collect();

    let mut count = 0;
    for task in tasks
        .iter()
        .filter(|t| !t.completed && norg_done.contains(&t.id))
    {
        log::info!("Marking '{title}' as done.", title = task.title);
        task_complete(auth.clone(), tasklist, &task.id).await?;
        count += 1;
    }

    Ok(count)
}

// Insert unkown remote tasks into source_code, BUT NOT the list of todos. Returns list of pulled
// tasks.
// Write to disk and reparse to get new tasks.
// Does not write to disk.
fn sync_pull_new(tasks: &[Task], norg: &mut ParsedNorg) -> Result<usize, Error> {
    let norg_ids: HashSet<Arc<str>> = norg.todos.iter().filter_map(|t| t.id.clone()).collect();

    let mut lines = norg.lines();

    let tasks_to_create = tasks
        .iter()
        .filter(|t| !t.completed && !norg_ids.contains(&t.id));

    let mut count = 0;
    for (i, task) in tasks_to_create.enumerate() {
        let line_to_insert = match (
            norg.todos.is_empty(),
            norg.line_number.todo_section,
            norg.line_number.section_after_todo,
        ) {
            (false, usize::MAX, usize::MAX) => norg.todos.last().unwrap().line + 1,
            (false, section_todo, section_next) => {
                norg.todos
                    .iter()
                    .filter(|t| section_todo < t.line && t.line < section_next)
                    .last()
                    .map(|t| t.line)
                    .unwrap_or(section_todo)
                    + 1
                    + i
            }
            (true, usize::MAX, usize::MAX) => lines.len(),
            (true, section_todo, _) => section_todo + 1 + i,
        };

        let title = task.title.clone();
        let id = task.id.clone();

        lines.insert(
            line_to_insert,
            format!("  - ( ) {title} %#taskid {id}%").into_bytes(),
        );
        count += 1;
    }
    norg.set_lines(&lines[..])?;

    Ok(count)
}

// Create unknown task and update the source code to contain the task ids.
// Returns newly created tasks.
// Does not write to disk.
async fn sync_push_new(
    auth: Authenticator,
    tasklist: &str,
    norg: &mut ParsedNorg,
) -> Result<Vec<Task>, Error> {
    let mut lines = norg.lines();

    let todo_to_create: Vec<&mut Todo> = norg
        .todos
        .iter_mut()
        .filter(|t| t.state == State::Undone && t.id.is_none())
        .collect();
    if todo_to_create.is_empty() {
        return Ok(Vec::new());
    }

    let mut new_tasks = Vec::new();
    for todo in todo_to_create {
        new_tasks.push(task_create(auth.clone(), tasklist, todo).await?);
        todo.append_id(&mut lines[todo.line]);
    }
    norg.set_lines(&lines[..])?;
    Ok(new_tasks)
}

// Check for undone todos with ID that do not have a corresponding task remote.
fn check_missing_remote_tasks<'a>(tasks: &[Task], norg: &'a ParsedNorg) -> Vec<&'a Todo> {
    let task_ids = tasks.iter().map(|t| t.id.clone()).collect::<HashSet<_>>();
    norg.todos
        .iter()
        .filter(|t| {
            t.state == State::Undone && t.id.is_some() && !task_ids.contains(t.id.as_ref().unwrap())
        })
        .collect::<Vec<_>>()
}

fn warn_missing_remote_tasks<'a, I: IntoIterator<Item = &'a Todo>>(filename: &Path, missing: I) {
    for m in missing.into_iter() {
        let file = filename.display();
        let task = m.content.clone();
        log::warn!("{file}: task '{task}' unexpectedly deleted from Google Tasks. Sync with --fix-missing to re-create.");
    }
}
