use google_tasks1::api::Task;
use google_tasks1::api::TaskList;
use google_tasks1::TasksHub;
use hyper::client::HttpConnector;
use hyper_rustls::HttpsConnector;

use crate::auth::Authenticator;
use crate::error::Error;
use crate::error::WrapError;

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

pub async fn get_tasks(auth: Authenticator, tasklist: &str) -> Result<(), Error> {
    let hub = create_hub(auth);

    let (_response, tasks) = hub
        .tasks()
        .list(tasklist)
        .doit()
        .await
        .during("get tasks")?;

    log::info!("{:#?}", tasks.items.unwrap());

    Ok(())
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
