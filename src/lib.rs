#[macro_use]
extern crate lazy_static;

use dashmap::DashMap;
use rand::Rng;
use reqwest::header::HeaderValue;
use serde_json::{json, Value as SerdeValue};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;

mod value;
use value::{Value, ValueError};

#[derive(Clone, Debug)]
pub struct Device {
    url: String,
    health: String,
    client: reqwest::Client,

    cache: Arc<Mutex<HashMap<String, Value>>>,

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
            health: format!("http://{}:{}/apiversion", hostname, port),
            client: reqwest::Client::new(),

            cache: Arc::new(Mutex::new(HashMap::new())),

            client_id: rng.gen::<u32>(),
            etag: None,
        }
    }

    pub async fn connect(&self) -> Result<(), DeviceError> {
        self.check().await?;

        let c = self.client.clone();
        let url = self.url.clone();
        let mut etag: Option<HeaderValue> = None;
        let cache = self.cache.clone();

        tokio::spawn(async move {
            loop {
                Self::client(&c, &url, &mut etag, &cache).await;
            }
        });

        Ok(())
    }

    async fn check(&self) -> Result<(), DeviceError> {
        match self
            .client
            .get(&self.health)
            .send()
            .await?
            .error_for_status()
        {
            Ok(_) => Ok(()),
            Err(_) => Err(DeviceError::CouldNotConnect(self.url.to_string())),
        }
    }

    async fn client(
        c: &reqwest::Client,
        url: &str,
        etag: &mut Option<HeaderValue>,
        cache: &Arc<Mutex<HashMap<String, Value>>>,
    ) -> Result<(), DeviceError> {
        // Check if we are long polling
        // If we are long polling, send the etag header we stored
        // If not just ask for new data

        let q = match &etag {
            Some(v) => c.get(url).header("If-None-Match", v).send().await?,
            None => c.get(url).send().await?,
        };

        // Store the "etag" value which we use to only get updates
        *etag = q.headers().get(reqwest::header::ETAG).cloned();

        // Map into hashmap
        let m = q.json::<HashMap<String, SerdeValue>>().await?;

        let c = cache.clone();
        let mut c = c.lock().await;

        for item in m.into_iter() {
            let v = Value::try_from(item.1)?.decode(&item.0)?;
            c.insert(item.0, v);
        }

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
    CouldNotConnect(String),
    #[error(transparent)]
    ValueParsingError(#[from] ValueError),
}
