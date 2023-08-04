use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

use crate::auth::Authenticator;
use crate::cfg::CFG;
use crate::opts::Sync as SyncOpts;
use crate::parse::{ParsedNorg, State, Todo};
use crate::tasks::{get_tasks, task_mark_completed, todo_create, Task};
use crate::Error;

pub async fn perform_sync(auth: Authenticator, opts: &SyncOpts) -> Result<(), Error> {
    let tasklist: Rc<str> = CFG.tasklist.as_str().into();

    let mut tasks = get_tasks(auth.clone(), &tasklist).await?;

    let original_tasks = tasks.clone();

    let mut todos = Vec::new();

    let idx_last = opts.files.len() - 1;
    for (i, file) in opts.files.iter().enumerate() {
        // Skip the file we want to pull to
        match (i, opts.pull_to_first) {
            (0, true) => continue,
            (idx, false) if idx == idx_last => continue,
            _ => {}
        }

        let syncer = Syncer {
            pull_completed: !opts.without_local,
            push_completed: !opts.without_remote,
            pull_new: false,
            push_new: !opts.without_remote && !opts.without_push,
            tasklist: tasklist.clone(),
        };

        let result = syncer.perform(auth.clone(), file, &tasks[..]).await?;
        tasks.extend(result.tasks_new);
        todos.extend(result.todos_present);
        println!(
            "{file}: {stats}",
            file = file.display(),
            stats = result.stats
        );
    }

    // Sync file that we pull to
    let present_todo_ids: Vec<Rc<str>> = todos.iter().filter_map(|t| t.id.clone()).collect();
    // tasks that were actually created new
    let new_remote_tasks = original_tasks
        .iter()
        .filter(|t| !present_todo_ids.contains(&t.id))
        .cloned()
        .collect::<Vec<_>>();

    let file_to_pull = &opts.files[if opts.pull_to_first { 0 } else { idx_last }];

    let result = Syncer {
        pull_completed: !opts.without_local,
        push_completed: !opts.without_remote,
        pull_new: !opts.without_local && !opts.without_pull,
        push_new: !opts.without_remote && !opts.without_push,
        tasklist: tasklist.clone(),
    }
    .perform(auth.clone(), file_to_pull, &new_remote_tasks[..])
    .await?;

    println!(
        "{file}: {stats}",
        file = file_to_pull.display(),
        stats = result.stats
    );

    Ok(())
}

struct Syncer {
    pull_completed: bool,
    push_completed: bool,
    pull_new: bool,
    push_new: bool,

    tasklist: Rc<str>,
}

#[derive(Debug, Clone)]
struct SyncStats {
    num_pull_completed: usize,
    num_push_completed: usize,
    num_pull_new: usize,
    num_push_new: usize,
}

impl std::fmt::Display for SyncStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pulled {pull_completed} completed, pushed {push_completed} completed, pulled {pull_new} new, pushed {push_new} new tasks",
        pull_completed=self.num_pull_completed,
        push_completed=self.num_push_completed,
        pull_new=self.num_pull_new,
        push_new=self.num_push_new,
        )
    }
}

#[derive(Debug, Clone)]
struct SyncResult {
    tasks_new: Vec<Task>,
    todos_present: Vec<Todo>,
    stats: SyncStats,
}

impl Syncer {
    // Perform full sync, returning newly created tasks
    pub async fn perform(
        &self,
        auth: Authenticator,
        file: &Path,
        tasks: &[Task],
    ) -> Result<SyncResult, Error> {
        let mut norg = ParsedNorg::parse(file)?;

        let mut tasks_new: Vec<Task> = Vec::new();

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
        log::trace!("Pre-push new:\n{norg:#?}");
        if self.push_new {
            let pushed = sync_push_new(auth.clone(), &self.tasklist, &mut norg).await?;
            num_push_new = pushed.len();
            tasks_new.extend(pushed);
        }

        norg.write()?;
        Ok(SyncResult {
            tasks_new,
            todos_present: norg.todos,
            stats: SyncStats {
                num_pull_completed,
                num_push_completed,
                num_pull_new,
                num_push_new,
            },
        })
    }
}

// Sync completed tasks from remote to neorg
fn sync_pull_completed(tasks: &[Task], norg: &mut ParsedNorg) -> Result<usize, Error> {
    let remote_done: HashSet<Rc<str>> = tasks
        .iter()
        .filter_map(|t| {
            if t.completed {
                Some(t.id.clone())
            } else {
                None
            }
        })
        .collect();

    let mut count = 0;
    for todo in norg.todos.iter_mut().filter(|t| {
        t.state != State::Done && t.id.is_some() && remote_done.contains(&t.id.clone().unwrap())
    }) {
        todo.state = State::Done;

        let len_state = todo.bytes.state_end - todo.bytes.state_start;
        if len_state != 1 {
            log::warn!(
                "expected single byte for state char, found {} bytes",
                len_state
            );
        }

        norg.source_code.splice(
            todo.bytes.state_start..todo.bytes.state_end,
            "x".as_bytes().iter().cloned(),
        );
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
    let norg_done: HashSet<Rc<str>> = norg
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
        task_mark_completed(auth.clone(), tasklist, &task.id).await?;
        count += 1;
    }

    Ok(count)
}

// Insert unkown remote tasks into source_code, BUT NOT the list of todos. Returns list of pulled
// tasks.
// Write to disk and reparse to get new tasks.
// Does not write to disk.
fn sync_pull_new(tasks: &[Task], norg: &mut ParsedNorg) -> Result<usize, Error> {
    let norg_ids: HashSet<Rc<str>> = norg.todos.iter().filter_map(|t| t.id.clone()).collect();

    let mut lines = norg.lines();

    let tasks_to_create = tasks
        .iter()
        .filter(|t| !t.completed && !norg_ids.contains(&t.id));

    let mut count = 0;
    for (i, task) in tasks_to_create.enumerate() {
        let line_to_insert = match (
            norg.todos.is_empty(),
            norg.line_no.todo_section,
            norg.line_no.section_after_todo,
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
            format!(" - ( ) {title} %#taskid {id}%").into_bytes(),
        );
        count += 1;
    }
    norg.source_code = lines.join("\n".as_bytes());

    Ok(count)
}

// Create unknown task and update the source code to contain the task ids.
// Returns newly created tasks.
// Does not write to disk.
pub async fn sync_push_new(
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
        new_tasks.push(todo_create(auth.clone(), tasklist, todo).await?);
        todo.append_id(&mut lines[todo.line]);
    }

    norg.source_code = lines.join("\n".as_bytes());

    Ok(new_tasks)
}
