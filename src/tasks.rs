use chrono::DateTime;
use chrono::Duration;
use chrono::Local;
use chrono::NaiveDate;
use google_tasks1::api::Task as GTask;
use google_tasks1::api::TaskList;
use google_tasks1::TasksHub;
use hyper::client::HttpConnector;
use hyper_rustls::HttpsConnector;
use indicatif::ProgressIterator;
use std::sync::Arc;

use crate::auth::Authenticator;
use crate::error::Error;
use crate::error::WrapError;
use crate::parse::Todo;
use crate::progress_bar::style_progress_bar_count;

#[derive(Debug, Clone)]
pub struct Task {
    pub completed: bool,
    pub id: Arc<str>,
    pub title: Arc<str>,
    pub modified_at: DateTime<Local>,
    pub due_at: Option<NaiveDate>,
}

impl TryFrom<&GTask> for Task {
    type Error = Error;

    fn try_from(task: &GTask) -> Result<Task, Error> {
        Ok(Task {
            completed: task.completed.is_some(),
            id: task
                .id
                .as_ref()
                .ok_or_else(|| Error::NotFound {
                    what: "task id".into(),
                })?
                .as_str()
                .into(),
            title: task
                .title
                .as_ref()
                .map(|s| s.as_str().into())
                .unwrap_or_else(|| Arc::from(String::new())),
            modified_at: DateTime::parse_from_rfc3339(
                task.updated.as_ref().expect("no updated time"),
            )?
            .into(),
            due_at: task
                .due
                .as_ref()
                .and_then(|d| DateTime::parse_from_rfc3339(d).ok().map(|d| d.date_naive())),
        })
    }
}

fn create_hub(auth: Authenticator) -> TasksHub<HttpsConnector<HttpConnector>> {
    TasksHub::new(
        hyper::Client::builder().build(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .https_only()
                .enable_http2()
                .build(),
        ),
        auth,
    )
}
pub async fn get_single_task(
    auth: Authenticator,
    tasklist: &str,
    task: &str,
) -> Result<GTask, Error> {
    let hub = create_hub(auth);
    let (_response, task) = hub
        .tasks()
        .get(tasklist, task)
        .doit()
        .await
        .during("get task")?;
    Ok(task)
}

pub async fn get_tasklists(auth: Authenticator) -> Result<Vec<TaskList>, Error> {
    let hub = create_hub(auth);

    let (response, tasklists) = hub
        .tasklists()
        .list()
        .doit()
        .await
        .during("getting task lists")?;

    log::debug!("got response:\n{response:#?}");

    tasklists.items.ok_or(Error::NotFound {
        what: "tasklists".into(),
    })
}

// Returns list of kept tasks and number of deleted tasks
pub async fn clear_tasks(
    auth: Authenticator,
    tasklist: &str,
    tasks: Vec<Task>,
    cutoff: Duration,
) -> Result<(Vec<Task>, usize), Error> {
    let hub = create_hub(auth);

    if log::log_enabled!(log::Level::Debug) {
        for t in tasks.iter() {
            log::debug!(
                "[{completed}] @{modified_at}: {title}",
                completed = if t.completed { "x" } else { " " },
                title = t.title,
                modified_at = t.modified_at.with_timezone(&chrono::Local)
            );
        }
    }

    let (delete, keep): (Vec<_>, Vec<_>) = tasks
        .into_iter()
        .partition(|t| t.completed && t.modified_at < Local::now() - cutoff);

    for task in delete
        .iter()
        .progress_with_style(style_progress_bar_count())
        .with_message(format!(
            "Clearing completed tasks older than {} days…",
            cutoff.num_days()
        ))
    {
        hub.tasks().delete(tasklist, &task.id).doit().await?;
    }
    Ok((keep, delete.len()))
}

pub async fn get_tasks(auth: Authenticator, tasklist: &str) -> Result<Vec<Task>, Error> {
    let hub = create_hub(auth);

    let mut tasks = Vec::new();
    let mut page_token: Option<String> = None;
    loop {
        let req = hub
            .tasks()
            .list(tasklist)
            .show_completed(true)
            .show_hidden(true);

        let req = if let Some(token) = page_token {
            req.page_token(&token)
        } else {
            req
        };
        let (_response, got_tasks) = req.doit().await.during("get tasks")?;

        page_token = got_tasks.next_page_token;

        if log::log_enabled!(log::Level::Debug)
            && got_tasks
                .items
                .as_ref()
                .is_some_and(|items| !items.is_empty())
        {
            log::debug!(
                "Got {num} tasks:",
                num = got_tasks.items.as_ref().unwrap().len()
            );
            for (i, t) in got_tasks.items.as_ref().unwrap().iter().enumerate() {
                log::debug!("Google Task #{}: {:#?}", i + 1, t);
            }
        }

        tasks.extend(
            got_tasks
                .items
                .ok_or_else(|| Error::NoTasks)?
                .iter()
                .map(Task::try_from)
                .collect::<Result<Vec<Task>, Error>>()?
                .into_iter(),
        );
        if page_token.is_none() {
            break;
        }
    }

    log::debug!("{:#?}", tasks);

    Ok(tasks)
}

pub async fn task_complete(auth: Authenticator, tasklist: &str, task: &str) -> Result<(), Error> {
    let mut gtask = get_single_task(auth.clone(), tasklist, task).await?;

    if gtask.completed.is_some() {
        log::warn!(
            "Task already completed: {}",
            gtask.title.as_deref().unwrap_or(task)
        )
    }
    gtask.status = Some("completed".into());

    let hub = create_hub(auth);

    hub.tasks()
        .update(gtask, tasklist, task)
        .doit()
        .await
        .during("setting task done")?;

    Ok(())
}

pub async fn task_create(
    auth: Authenticator,
    tasklist: &str,
    todo: &mut Todo,
) -> Result<Task, Error> {
    let hub = create_hub(auth);
    let req = GTask {
        title: Some(todo.content.to_string()),
        due: todo.due_at_fmt(),
        ..GTask::default()
    };
    let (_response, task) = hub
        .tasks()
        .insert(req, tasklist)
        .doit()
        .await
        .during("creating task")?;

    todo.id = task.id.as_ref().map(|s| Arc::from(s.as_str()));
    Task::try_from(&task)
}

pub async fn task_update(auth: Authenticator, tasklist: &str, todo: &Todo) -> Result<GTask, Error> {
    if todo.id.is_none() {
        return Err(Error::TodoNoID {
            content: todo.content.to_string(),
        });
    }

    let mut gtask = get_single_task(auth.clone(), tasklist, todo.id.as_ref().unwrap()).await?;

    gtask.title = Some(todo.content.to_string());
    gtask.due = todo.due_at_fmt();

    let hub = create_hub(auth);

    let (_response, task) = hub
        .tasks()
        .update(gtask, tasklist, todo.id.as_ref().unwrap())
        .doit()
        .await
        .during_f(|| {
            format!(
                "updaing task: [{id}] {content}",
                id = todo.id.as_ref().unwrap(),
                content = todo.content
            )
            .into()
        })?;

    Ok(task)
}

pub fn print_tasklists(tasklists: &[TaskList]) -> Result<(), Error> {
    let maxlen = tasklists
        .iter()
        .map(|tl| tl.id.as_ref().map(|t| t.len()).unwrap_or(0))
        .max()
        .unwrap_or(0);
    for tl in tasklists.iter() {
        let id = tl.id.as_ref().ok_or(Error::NotFound {
            what: "id for tasklist".into(),
        })?;
        let title = tl.title.as_ref().ok_or(Error::NotFound {
            what: "title for tasklist".into(),
        })?;
        println!("{id:<maxlen$} {title}");
    }
    Ok(())
}
