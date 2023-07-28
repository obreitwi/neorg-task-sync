use std::collections::HashSet;
use std::rc::Rc;

use crate::auth::Authenticator;
use crate::parse::{ParsedNorg, State};
use crate::tasks::{task_mark_completed, Task};
use crate::Error;

// Sync completed tasks from remote to neorg
pub fn sync_completed_to_norg(tasks: &[Task], norg: &mut ParsedNorg) -> Result<(), Error> {
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
pub async fn sync_completed_from_norg(
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

pub fn sync_unknown(norg: &ParsedNorg) -> Result<(), Error> {
    todo!()
}
