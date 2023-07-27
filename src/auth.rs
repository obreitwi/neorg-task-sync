use hyper::client::HttpConnector;
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod};

use crate::cfg::clientsecret_name;
use crate::cfg::tokencache_name;
use crate::error::Error;
use crate::error::WrapError;

const SCOPES: [&str; 2] = [
    "https://www.googleapis.com/auth/tasks",
    "https://www.googleapis.com/auth/tasks.readonly",
];

pub type Authenticator =
    yup_oauth2::authenticator::Authenticator<hyper_rustls::HttpsConnector<HttpConnector>>;

pub async fn login() -> Result<Authenticator, Error> {
    // Read application secret from a file. Sometimes it's easier to compile it directly into
    // the binary. The clientsecret file contains JSON like `{"installed":{"client_id": ... }}`
    //
    log::debug!("reading client secret: {}", clientsecret_name());
    let secret = yup_oauth2::read_application_secret(clientsecret_name())
        .await
        .during("reading clientsecret")?;

    // Create an authenticator that uses an InstalledFlow to authenticate. The
    // authentication tokens are persisted to a file named tokencache.json. The
    // authenticator takes care of caching tokens to disk and refreshing tokens once
    // they've expired.
    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
        .persist_tokens_to_disk(tokencache_name())
        .build()
        .await
        .during("creating authenticator")?;

    // token(<scopes>) is the one important function of this crate; it does everything to
    // obtain a token that can be sent e.g. as Bearer token.
    let _ = auth.token(&SCOPES[..]).await.during("obtaining auth token");
    Ok(auth)
}
