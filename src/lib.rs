#[macro_use]
extern crate lazy_static;

use rand::Rng;
use reqwest::header::HeaderValue;
use serde_json::{json, Value as SerdeValue};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;

mod value;

#[derive(Clone, Debug)]
pub struct Device {
    url: String,
    client: reqwest::Client,

    cache: Arc<Mutex<HashMap<String, SerdeValue>>>,

    client_id: u32,
    etag: Option<HeaderValue>,
}

impl Device {
    pub async fn discover(name: &str, timeout: Option<Duration>) -> Result<Device, DiscoveryError> {
        // Default duration of 10 secs
        let timeout = match timeout {
            Some(v) => v,
            None => Duration::from_secs(10),
        };

        let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
        let mut services = browser.timeout(timeout).browse()?;

        while let Some(Ok(v)) = services.recv().await {
            if v.name() == name {
                let resolved_service = async_zeroconf::ServiceResolver::r(&v).await?;

                return Ok(Self::new(
                    &format!(
                        "{}.{}",
                        resolved_service
                            .host()
                            .as_ref()
                            .ok_or(DiscoveryError::NoHost)?,
                        resolved_service
                            .domain()
                            .as_ref()
                            .ok_or(DiscoveryError::NoDomain)?
                    ),
                    resolved_service.port(),
                ));
            }
        }

        Err(DiscoveryError::NoDeviceDiscovered(name.to_string()))
    }

    pub fn new(hostname: &str, port: u16) -> Device {
        let mut rng = rand::thread_rng();

        Device {
            url: format!("http://{}:{}/datastore", hostname, port),
            client: reqwest::Client::new(),

            cache: Arc::new(Mutex::new(HashMap::new())),

            client_id: rng.gen::<u32>(),
            etag: None,
        }
    }

    pub async fn rq(&mut self) -> Result<(), DeviceError> {
        // Check if we are long polling
        // If we are long polling, send the etag header we stored
        // If not just ask for new data

        let q = match &self.etag {
            Some(v) => {
                self.client
                    .get(&self.url)
                    .header("If-None-Match", v)
                    .send()
                    .await?
            }
            None => self.client.get(&self.url).send().await?,
        };

        // Store the "etag" value which we use to only get updates
        self.etag = q.headers().get(reqwest::header::ETAG).cloned();

        // Map into hashmap
        let m = q.json::<HashMap<String, Value>>().await?;

        // Replace our internal cache
        self.cache.clone().lock().await.extend(m.into_iter());

        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum DiscoveryError {
    #[error(transparent)]
    ZeroconfError(#[from] async_zeroconf::ZeroconfError),
    #[error("no host discovered?")]
    NoHost,
    #[error("no domain discovered?")]
    NoDomain,
    #[error("no device with name: `{0}` discovered")]
    NoDeviceDiscovered(String),
}

#[derive(Error, Debug)]
pub enum DeviceError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error("could not connect to device: `{0}`")]
    NoDeviceDiscovered(String),
}
