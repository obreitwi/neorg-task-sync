use google_tasks1::oauth2::authenticator::Authenticator as OAuthenticator;
use google_tasks1::oauth2::hyper::client::Client;
use google_tasks1::oauth2::read_application_secret;
use google_tasks1::oauth2::InstalledFlowAuthenticator;
use google_tasks1::oauth2::InstalledFlowReturnMethod;
use hyper::client::HttpConnector;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};

use crate::cfg::clientsecret_name;
use crate::cfg::tokencache_name;
use crate::error::Error;
use crate::error::WrapError;

const SCOPES: [&str; 2] = [
    "https://www.googleapis.com/auth/tasks",
    "https://www.googleapis.com/auth/tasks.readonly",
];

pub type Authenticator = OAuthenticator<HttpsConnector<HttpConnector>>;

pub async fn login() -> Result<Authenticator, Error> {
    // Read application secret from a file.
    log::debug!("reading client secret: {}", clientsecret_name());
    let secret = read_application_secret(clientsecret_name())
        .await
        .during("reading clientsecret")?;

    // Create the token cache folder if it doesn't exist.
    if let Some(token_folder) = std::path::PathBuf::from(tokencache_name()).parent() {
        if !token_folder.exists() {
            std::fs::create_dir_all(token_folder).during("creating folder for token")?;
        }
    }

    let connector = HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();

    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
        .persist_tokens_to_disk(tokencache_name())
        .hyper_client(Client::builder().build(connector))
        .build()
        .await
        .during("creating authenticator")?;

    let _ = auth.token(&SCOPES).await.during("obtaining auth token")?;

    Ok(auth)
}
