# vendor-posthog-client

Can be used either with env posthog api key or with Google cloud secret manager

## Usage

`POSTHOG_API_KEY` is required when using `ApiOptions::from_env()` or `ApiOptions::auto()`

`SERVICE_ACCOUNT` is required when using `ApiOptions::from_google_secret_manager()` or `ApiOptions::auto()`

`ApiOptions::auto()` will try to use `ApiOptions::from_env()` first and then `ApiOptions::from_google_secret_manager()`

```rust
use vendor_posthog_client::{ApiOptions, Client, Event};
use chrono::Utc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opts = ApiOptions::from_env();
    assert!(opts.is_ok());
    let opts = opts.unwrap();
    let client = Client::new(opts);
    
    // create event
    let mut event = Event::new("user_logged_in".to_string(), "distinct_id_user".to_string());
    // insert single property
    event.insert_prop("key".to_string(), "value".to_string());
    // insert multiple properties
    event.insert_prop_many(vec![
        ("key1".to_string(), "value1".to_string()),
        ("key2".to_string(), "value2".to_string()),
    ]);
    // set timestamp if needed
    event.set_timestamp(chrono::Utc::now().naive_utc());
    // send event
    client.capture(event).await.unwrap();
}
```