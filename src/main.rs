use google_tasks1::TasksHub;
use hyper;
use hyper_rustls;
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read application secret from a file. Sometimes it's easier to compile it directly into
    // the binary. The clientsecret file contains JSON like `{"installed":{"client_id": ... }}`
    let secret = yup_oauth2::read_application_secret("clientsecret.json")
        .await
        .expect("clientsecret.json");

    // Create an authenticator that uses an InstalledFlow to authenticate. The
    // authentication tokens are persisted to a file named tokencache.json. The
    // authenticator takes care of caching tokens to disk and refreshing tokens once
    // they've expired.
    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
        .persist_tokens_to_disk("tokencache.json")
        .build()
        .await
        .unwrap();

    let scopes = &[
        "https://www.googleapis.com/auth/tasks",
        "https://www.googleapis.com/auth/tasks.readonly",
    ];

    // token(<scopes>) is the one important function of this crate; it does everything to
    // obtain a token that can be sent e.g. as Bearer token.
    match auth.token(scopes).await {
        Ok(token) => println!("The token is {:?}", token),
        Err(e) => println!("error: {:?}", e),
    }

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

    let (response, tasklists) = hub.tasklists().list().doit().await?;

    println!("Got response:\n{response:#?}");

    if let Some(lists) = tasklists.items.as_ref() {
        for (i, list) in lists.iter().enumerate() {
            println!("#{idx}: {list:#?}", idx = i + 1);
        }
    }

    Ok(())
}
