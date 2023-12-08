use anyhow::{anyhow, Result};
use google_secretmanager1::hyper::client::HttpConnector;
use google_secretmanager1::hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use google_secretmanager1::{
    hyper, hyper_rustls,
    oauth2::{read_service_account_key, ServiceAccountAuthenticator},
    SecretManager,
};
use std::env;

pub async fn get_service_account_path() -> Result<String> {
    Ok(env::var("SERVICE_ACCOUNT")?)
}
pub struct GoogleSecretManager {
    client: SecretManager<HttpsConnector<HttpConnector>>,
}

impl GoogleSecretManager {
    pub async fn new() -> Result<GoogleSecretManager> {
        let service_account_path = get_service_account_path().await?;
        let service_account_key = read_service_account_key(&service_account_path)
            .await
            .expect("failed to read service account key");
        let auth = ServiceAccountAuthenticator::builder(service_account_key)
            .build()
            .await
            .expect("failed to create authenticator");

        Ok(GoogleSecretManager {
            client: SecretManager::new(
                hyper::Client::builder().build(
                    HttpsConnectorBuilder::new()
                        .with_native_roots()
                        .https_or_http()
                        .enable_http1()
                        .enable_http2()
                        .build(),
                ),
                auth,
            ),
        })
    }

    pub async fn get_secret(&self, project: &str, secret: &str) -> Result<Vec<u8>> {
        let secret_name = format!("projects/{project}/secrets/{secret}/versions/latest");
        let (_, s) = self
            .client
            .projects()
            .secrets_versions_access(&secret_name)
            .doit()
            .await?;

        let secret = if let Some(pl) = s.payload {
            if let Some(data) = pl.data {
                Ok(data)
            } else {
                Err(anyhow!("No data"))
            }
        } else {
            Err(anyhow!("No payload"))
        };

        secret
    }
}
