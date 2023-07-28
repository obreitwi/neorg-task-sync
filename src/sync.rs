use std::collections::HashSet;
use std::rc::Rc;

use crate::parse::{ParsedNorg, State};
use crate::tasks::Task;
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

    Ok(())
}

// Sync completed tasks from neorg to remote
pub fn sync_completed_from_norg(norg: &mut ParsedNorg, tasks: &[Task]) -> Result<(), Error> {
    let norg_done: HashSet<Rc<str>> = norg
        .todos
        .iter()
        .filter_map(|t| match (t.id.as_ref(), &t.state) {
            (Some(id), State::Done) => Some(id.clone()),
            _ => None,
        })
        .collect();

    Ok(())
}

pub fn sync_unknown(norg: &ParsedNorg) -> Result<(), Error> {
    todo!()
}
