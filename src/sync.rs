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
    let num_files = opts.files.len();

    let should_pull_new = |idx: usize| -> bool {
        if opts.pull_to_first {
            idx == 0
        } else {
            idx == num_files - 1
        }
    };

    for (i, file) in opts.files.iter().enumerate() {
        let syncer = Syncer {
            pull_completed: !opts.without_local,
            push_completed: !opts.without_remote,
            pull_new: !opts.without_local && !opts.without_pull && should_pull_new(i),
            push_new: !opts.without_remote && !opts.without_push,
            tasklist: CFG.tasklist.as_str().into(),
        };

        syncer.perform(auth.clone(), file).await?;
    }
    Ok(())
}

struct Syncer {
    pull_completed: bool,
    push_completed: bool,
    pull_new: bool,
    push_new: bool,

    tasklist: Rc<str>,
}

impl Syncer {
    // Perform full sync
    pub async fn perform(&self, auth: Authenticator, file: &Path) -> Result<(), Error> {
        let mut norg = ParsedNorg::parse(file)?;

        let tasks = get_tasks(auth.clone(), &self.tasklist).await?;

        if self.pull_completed {
            sync_pull_completed(&tasks[..], &mut norg)?;
        }
        if self.pull_new {
            sync_pull_new(&tasks[..], &mut norg)?;
        }

        if self.push_completed {
            sync_push_completed(auth.clone(), &self.tasklist, &mut norg, &tasks[..]).await?;
        }
        if self.push_new {
            sync_push_new(auth.clone(), &self.tasklist, &mut norg).await?;
        }

        norg.write()?;
        Ok(())
    }
}

// Sync completed tasks from remote to neorg
fn sync_pull_completed(tasks: &[Task], norg: &mut ParsedNorg) -> Result<(), Error> {
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

        norg.source_code = norg
            .source_code
            .splice(
                todo.bytes.state_start..todo.bytes.state_end,
                ['x' as u8].into_iter(),
            )
            .collect();
    }
    Ok(())
}

// Sync completed tasks from neorg to remote
async fn sync_push_completed(
    auth: Authenticator,
    tasklist: &str,
    norg: &mut ParsedNorg,
    tasks: &[Task],
) -> Result<(), Error> {
    let norg_done: HashSet<Rc<str>> = norg
        .todos
        .iter()
        .filter_map(|t| match (t.id.as_ref(), &t.state) {
            (Some(id), State::Done) => Some(id.clone()),
            _ => None,
        })
        .collect();

    for task in tasks
        .iter()
        .filter(|t| !t.completed && norg_done.contains(&t.id))
    {
        task_mark_completed(auth.clone(), tasklist, &task.id).await?;
    }

    Ok(())
}

// Insert unkown remote tasks into source_code, BUT NOT the list of todos.
// Write to disk and reparse.
// Does not write to disk.
fn sync_pull_new(tasks: &[Task], norg: &mut ParsedNorg) -> Result<(), Error> {
    let norg_ids: HashSet<Rc<str>> = norg.todos.iter().filter_map(|t| t.id.clone()).collect();

    let mut lines = norg.lines();

    let tasks_to_create = tasks
        .iter()
        .filter(|t| !t.completed && !norg_ids.contains(&t.id));

    for (i, task) in tasks_to_create.enumerate() {
        let line_to_insert = match (norg.todos.is_empty(), norg.line_no_todo_section) {
            (false, _) => norg.todos.last().unwrap().line + 1,
            (true, usize::MAX) => lines.len(),
            (true, line) => line + 1 + i,
        };

        let title = task.title.clone();
        let id = task.id.clone();

        lines.insert(
            line_to_insert,
            format!("  - ( ) {title} %#taskid {id}%").into_bytes(),
        )
    }
    norg.source_code = lines.join("\n".as_bytes());

    Ok(())
}

// Create unknown task and update the source code to contain the task ids.
// Does not write to disk.
pub async fn sync_push_new(
    auth: Authenticator,
    tasklist: &str,
    norg: &mut ParsedNorg,
) -> Result<(), Error> {
    let mut lines = norg.lines();

    let todo_to_create: Vec<&mut Todo> = norg
        .todos
        .iter_mut()
        .filter(|t| t.state == State::Undone && t.id.is_none())
        .collect();
    if todo_to_create.is_empty() {
        return Ok(());
    }

    for todo in todo_to_create {
        todo_create(auth.clone(), tasklist, todo).await?;
        todo.append_id(&mut lines[todo.line]);
    }

    norg.source_code = lines.join("\n".as_bytes());

    Ok(())
}
