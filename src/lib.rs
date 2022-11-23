#[macro_use]
extern crate lazy_static;

use dashmap::DashMap;
use rand::Rng;
use reqwest::{header::HeaderValue, StatusCode};
use serde_json::Value as SerdeValue;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc::{channel, Sender};

mod value;
pub use value::{Value, ValueError};
pub mod definitions;

#[derive(Clone, Debug)]
pub struct Device {
    url: String,
    health: String,
    client: reqwest::Client,

    conn_cancel: Option<Sender<()>>,

    cache: Arc<DashMap<String, Value>>,

    client_id: u32,
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

            conn_cancel: None,

            cache: Arc::new(DashMap::new()),

            client_id: rng.gen::<u32>(),
        }
    }

    pub async fn connect(&mut self) -> Result<(), DeviceError> {
        self.check().await?;

        let (tx, mut rx) = channel(1);
        self.conn_cancel = Some(tx);

        let c = self.client.clone();
        let url = self.url.clone();
        let mut etag: Option<HeaderValue> = None;
        let cache = self.cache.clone();
        let client_id = self.client_id;

        // Start background long polling
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // poll
                    res = Self::poll(&c, &url, &mut etag, client_id, &cache) => {
                        if let Err(e) = res {
                            // TODO sort this error handling out
                            println!("{:?}", e);
                            return;
                        }
                    }

                    // exit if we cancel
                    _ = rx.recv() => {
                        return;
                    }
                }
            }
        });

        Ok(())
    }

    pub fn get(&self) -> Arc<DashMap<String, Value>> {
        self.cache.clone()
    }

    /// Simple method to search for a key, basiclaly .contains() helper for the backing map
    pub fn find(&self, key: &str) -> Vec<(String, Value)> {
        self.cache
            .iter()
            .filter(|f| f.key().contains(key))
            .map(|vk| (vk.key().clone(), vk.value().clone()))
            .collect()
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

    async fn poll(
        c: &reqwest::Client,
        url: &str,
        etag: &mut Option<HeaderValue>,
        client_id: u32,
        cache: &Arc<DashMap<String, Value>>,
    ) -> Result<(), DeviceError> {
        // Check if we are long polling
        // If we are long polling, send the etag header we stored
        // If not just ask for new data

        let c = c.get(url).query(&[("client", client_id)]);

        let q = match &etag {
            Some(v) => c.header("If-None-Match", v).send().await?,
            None => c.send().await?,
        };

        // Return early if the content hasn't been modified
        if q.status() == 304 {
            return Ok(());
        }

        // Store the "etag" value which we use to only get updates
        *etag = q.headers().get(reqwest::header::ETAG).cloned();

        // Map into hashmap
        let m = q.json::<HashMap<String, SerdeValue>>().await?;

        let c = cache.clone();

        for item in m.into_iter() {
            let v = Value::try_from(item.1)?.decode(&item.0)?;
            c.insert(item.0, v);
        }

        Ok(())
    }

    pub async fn set(&self, data: &[(&str, Value)]) -> Result<(), DeviceError> {
        let mut m = HashMap::new();

        for (key, val) in data.iter() {
            m.insert(key.to_string(), val.clone());
        }

        let form = reqwest::multipart::Form::new().text("json", serde_json::to_string(&m)?);

        let res = self
            .client
            .patch(&self.url)
            .query(&[("client", self.client_id)])
            .multipart(form)
            .send()
            .await?;

        match res.status() {
            StatusCode::OK | StatusCode::NO_CONTENT => {
                for (key, val) in data.into_iter() {
                    self.cache.insert(key.to_string(), val.clone());
                }
                Ok(())
            }
            _ => Err(DeviceError::BadResponse(
                res.status().clone(),
                res.text().await?,
            )),
        }
    }

    pub fn get_value(&self, key: &str) -> Option<Value> {
        match self.cache.get(key) {
            Some(v) => Some(v.value().clone()),
            None => None,
        }
    }

    pub fn uid(&self) -> Option<String> {
        self.get_value("uid").map(Into::into)
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
    #[error(transparent)]
    SerializationError(#[from] serde_json::Error),
    #[error("could not connect to device: `{0}`")]
    CouldNotConnect(String),
    #[error(transparent)]
    ValueParsingError(#[from] ValueError),
    #[error("unexpected response from device: `{0}`: `{1}`")]
    BadResponse(StatusCode, String),
}
