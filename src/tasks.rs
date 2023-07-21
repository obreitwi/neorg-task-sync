use google_tasks1::TasksHub;
use hyper;
use hyper_rustls;

use crate::auth::Authenticator;
use crate::error::Error;
use crate::error::WrapError;

async fn get_tasklists(auth: Authenticator) -> Result<(), Error> {
    // TODO: move to own function
    let hub = TasksHub::new(
        hyper::Client::builder().build(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .https_only()
                .enable_http2()
                .build(),
        ),
        auth,
    );

    let (response, tasklists) = hub
        .tasklists()
        .list()
        .doit()
        .await
        .during("getting task lists")?;

    println!("Got response:\n{response:#?}");

    if let Some(lists) = tasklists.items.as_ref() {
        for (i, list) in lists.iter().enumerate() {
            println!("#{idx}: {list:#?}", idx = i + 1);
        }
    }

    Ok(())
}
