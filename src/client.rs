use anyhow::{anyhow, Result};
use serde::{Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::OnceCell;
use tokio::time::timeout;
use aws_config::{load_defaults, BehaviorVersion};
use hyper::{
        header::CONTENT_TYPE,
        client::HttpConnector,
        Request,
        self
};
use hyper_tls::HttpsConnector;


const API_ENDPOINT: &str = "https://app.posthog.com/";
const APT_CAPTURE: &str = "capture/";
const TIMEOUT: Duration = Duration::from_millis(2000);
const POSTHOG_ENV: &str = "POSTHOG_API_KEY";


static HYPER_CLIENT: OnceCell<hyper::Client<HttpsConnector<HttpConnector>>> = OnceCell::const_new();

async fn init_hyper_client() -> hyper::Client<HttpsConnector<HttpConnector>> {
    let https = HttpsConnector::new();

    hyper::client::Client::builder().build::<_, hyper::Body>(https)
}

#[derive(Debug, Clone)]
pub struct ApiOptions {
    endpoint: String,
    key: String,
}

#[derive(Debug, Clone)]
pub struct Client {
    options: ApiOptions,
    timeout: Duration,
}

#[derive(serde::Serialize, Debug, PartialEq, Eq)]
pub struct Event {
    event: String,
    properties: Properties,
    timestamp: Option<chrono::NaiveDateTime>,
}

#[derive(serde::Serialize, Debug, PartialEq, Eq, Clone)]
pub struct Properties {
    distinct_id: String,
    #[serde(flatten)]
    properties: HashMap<String, String>,
}

#[derive(Serialize, Debug)]
struct InnerEvent {
    api_key: String,
    event: String,
    properties: Properties,
    timestamp: Option<chrono::NaiveDateTime>,
}

impl ApiOptions {
    pub fn new(endpoint: String, key: String) -> ApiOptions {
        ApiOptions { endpoint, key }
    }

    pub fn from_env() -> Result<ApiOptions> {
        let key = std::env::var(POSTHOG_ENV)?;
        assert!(!key.trim().is_empty());

        Ok(ApiOptions::new(API_ENDPOINT.to_string(), key))
    }

    pub async fn from_aws_secret_manager(secret: &str) -> Result<ApiOptions> {
        let cfg = load_defaults(BehaviorVersion::latest()).await;
        let client = aws_sdk_secretsmanager::Client::new(&cfg);
        
        let key = match client
            .get_secret_value()
            .secret_id(secret)
            .send()
            .await
            {
                Ok(s) => {
                    if let Some(s) = s.secret_string {
                        s
                    } else {
                        return Err(anyhow!("No Secret found for given posthog key"))
                    }
                }
                Err(e) => {
                    return Err(anyhow!("error getting secret: {e:?}"));
                }
            };

        assert!(!key.trim().is_empty());

        Ok(ApiOptions::new(API_ENDPOINT.to_string(), key))
    }

    pub async fn auto(secret: &str) -> Result<ApiOptions> {
        match ApiOptions::from_env() {
            Ok(options) => Ok(options),
            Err(_) => match ApiOptions::from_aws_secret_manager(secret).await {
                Ok(options) => Ok(options),
                Err(e) => Err(e),
            },
        }
    }
}

impl Client {
    pub fn new(options: ApiOptions) -> Client {
        Client { options , timeout: TIMEOUT}
    }

    pub async fn new_with_timeout(options: ApiOptions, timeout: Duration) -> Client {
        Client { options, timeout }
    }

    pub async fn capture(&self, event: Event) -> Result<()> {
        let client = HYPER_CLIENT.get_or_init(init_hyper_client).await;
        let inner_event = InnerEvent::new(event, self.options.key.clone());
        let url = format!("{}{}", self.options.endpoint, APT_CAPTURE);

        let request = Request::builder()
            .method("POST")
            .uri(url)
            .header(CONTENT_TYPE, "application/json")
            .body(hyper::Body::from(serde_json::to_string(&inner_event)?))?;

        let future = client.request(request);
        let _response = match timeout(self.timeout, future).await {
            Ok(response) => response,
            Err(e) => {
                return Err(anyhow::anyhow!("Error: {}", e));
            }
        };


        Ok(())
    }

    pub async fn capture_batch(&self, events: Vec<Event>) -> Result<()> {
        for event in events {
            self.capture(event).await?;
        }

        Ok(())
    }
}

impl Event {
    pub fn new<T: Into<String>>(event: T, distinct_id: T) -> Event {
        Event {
            event: event.into(),
            properties: Properties::new(distinct_id.into()),
            timestamp: None,
        }
    }

    pub fn insert_prop<T: Into<String>>(&mut self, key: T, value: T) {
        self.properties.insert(key.into(), value.into());
    }

    pub fn insert_prop_many<T: Into<String>>(&mut self, props: Vec<(T, T)>) {
        props.into_iter().for_each(|(key, value)| {
            self.properties.insert(key.into(), value.into());
        });
    }

    pub fn set_timestamp(&mut self, timestamp: chrono::NaiveDateTime) {
        self.timestamp = Some(timestamp);
    }
}

impl InnerEvent {
    pub fn new(event: Event, api_key: String) -> InnerEvent {
        InnerEvent {
            api_key,
            event: event.event.to_lowercase(),
            properties: event.properties,
            timestamp: event.timestamp,
        }
    }
}

impl Properties {
    pub fn new(distinct_id: String) -> Properties {
        Properties {
            distinct_id,

            properties: HashMap::default(),
        }
    }

    pub fn insert(&mut self, key: String, value: String) {
        self.properties.insert(key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json;

    async fn test_client(client: &Client) {
        let mut event = Event::new("TEST_EVENT".to_string(), "distinct_id_username_flatten_test".to_string());
        event.insert_prop("test_key".to_string(), "test_value".to_string());
        event.insert_prop_many(vec![
            ("test_key1".to_string(), "test_value1".to_string()),
            ("test_key2".to_string(), "test_value2".to_string()),
        ]);
        event.set_timestamp(chrono::Utc::now().naive_utc());
        client.capture(event).await.unwrap();
    }

    #[test]
    #[ignore]
    fn inner_event_serializes() {
        let mut event = Event::new("event".to_string(), "distinct_id".to_string());
        event.insert_prop("key".to_string(), "value".to_string());
        let inner_event = InnerEvent::new(event, "api_key".to_string());
        let json = serde_json::to_value(&inner_event).unwrap();
        let assert_json = "{\"api_key\":\"api_key\",\"event\":\"event\",\"properties\":{\"distinct_id\":\"distinct_id\",\"properties\":{\"key\":\"value\"}},\"timestamp\":null}";
        assert_eq!(json, assert_json.parse::<serde_json::Value>().unwrap());
    }

    #[tokio::test]
    async fn test_client_env() {
        let opts = ApiOptions::from_env();
        assert!(opts.is_ok());
        let opts = opts.unwrap();
        let client = Client::new(opts);
        test_client(&client).await;
    }

    #[tokio::test]
    async fn test_client_aws_secret_manager() {
        let secret = std::env::var("SECRET").unwrap();
        
        let opts = ApiOptions::from_aws_secret_manager(secret.as_str()).await;
        if opts.is_err() {
            panic!("Error: {}", opts.err().unwrap());
        }

        let opts = opts.unwrap();
        let client = Client::new(opts);
        test_client(&client).await;
    }
}
