use chrono::DateTime;
use chrono::Utc;
use google_tasks1::api::Task as GTask;
use google_tasks1::api::TaskList;
use google_tasks1::TasksHub;
use hyper::client::HttpConnector;
use hyper_rustls::HttpsConnector;
use std::sync::Arc;

use crate::auth::Authenticator;
use crate::error::Error;
use crate::error::WrapError;
use crate::parse::Todo;

#[derive(Debug, Clone)]
pub struct Task {
    pub completed: bool,
    pub id: Arc<str>,
    pub title: Arc<str>,
    pub modified_at: DateTime<Utc>,
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

pub async fn task_update_title(
    auth: Authenticator,
    tasklist: &str,
    task: &str,
    title: &str,
) -> Result<GTask, Error> {
    let mut gtask = get_single_task(auth.clone(), tasklist, task).await?;

    gtask.title = Some(title.into());

    let hub = create_hub(auth);

    let (_response, task) = hub
        .tasks()
        .update(gtask, tasklist, task)
        .doit()
        .await
        .during("setting task done")?;

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
