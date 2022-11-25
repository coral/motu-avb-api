#[macro_use]
extern crate lazy_static;

use async_zeroconf::Service;
use dashmap::DashMap;
use extchannel::{ChannelBank, ParseError};
use rand::Rng;
use reqwest::{header::HeaderValue, StatusCode};
use serde_json::Value as SerdeValue;
use std::sync::Arc;
use std::time::Duration;
use std::{collections::HashMap, fmt::Display};
use thiserror::Error;
use tokio::sync::mpsc::{channel, Sender};

mod value;
pub use value::{Value, ValueError};
pub mod extchannel;

#[allow(dead_code)]
#[derive(Debug)]
pub struct Device {
    name: String,
    hostname: String,
    port: u16,

    connected: bool,

    url: String,
    health: String,
    device_type: DeviceType,
    client: reqwest::Client,

    conn_cancel: Option<Sender<()>>,

    cache: Arc<DashMap<String, Value>>,
    updates: Option<tokio::sync::broadcast::Sender<(String, Value)>>,

    input_banks: Option<Arc<DashMap<u32, ChannelBank>>>,
    output_banks: Option<Arc<DashMap<u32, ChannelBank>>>,

    client_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeviceType {
    Host,
    Device,
    Unknown,
}

impl From<&str> for DeviceType {
    fn from(value: &str) -> Self {
        match value {
            "netiodevice" => DeviceType::Device,
            "netiohost" => DeviceType::Host,
            _ => DeviceType::Unknown,
        }
    }
}

impl Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let t = match self {
            DeviceType::Host => "Host",
            DeviceType::Device => "Device",
            DeviceType::Unknown => "Unknown",
        };
        write!(f, "{}", t)
    }
}

impl Display for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Name: \"{}\"  Type: {}  Hostname: {}:{}",
            self.name,
            self.device_type.to_string(),
            self.hostname,
            self.port
        )
    }
}

impl PartialEq for Device {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.hostname == other.hostname
            && self.port == other.port
            && self.device_type == other.device_type
    }
}

impl Device {
    pub async fn from_name(
        name: &str,
        timeout: Option<Duration>,
    ) -> Result<Device, DiscoveryError> {
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

                return Self::new_from_mdns(&resolved_service);
            }
        }

        Err(DiscoveryError::NoDeviceWithNameDiscovered(name.to_string()))
    }

    pub async fn discover(timeout: Option<Duration>) -> Result<Vec<Device>, DiscoveryError> {
        // Default duration of 10 secs
        let timeout = match timeout {
            Some(v) => v,
            None => Duration::from_secs(10),
        };

        let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
        let mut services = browser.timeout(timeout).browse()?;

        let mut devices = Vec::new();

        while let Some(Ok(v)) = services.recv().await {
            let resolved_service = async_zeroconf::ServiceResolver::r(&v).await?;

            match resolved_service
                .txt()
                .iter()
                .find(|(k, _)| k.contains("motu.mdns.type"))
            {
                Some((_, v)) => {
                    let d = std::str::from_utf8(v)?;
                    if d.contains("netiodevice") || d.contains("netiohost") {
                        let nd = Self::new_from_mdns(&resolved_service)?;
                        if devices.iter().find(|v| **v == nd).is_none() {
                            devices.push(nd);
                        }
                    }
                }
                None => {}
            };
        }

        match devices.len() {
            0 => Err(DiscoveryError::NoDevice),
            _ => Ok(devices),
        }
    }

    pub fn new(name: &str, hostname: &str, port: u16, device_type: DeviceType) -> Device {
        let mut rng = rand::thread_rng();

        Device {
            name: name.to_string(),
            hostname: hostname.to_string(),
            port,

            connected: false,

            url: format!("http://{}:{}/datastore", hostname, port),
            health: format!("http://{}:{}/apiversion", hostname, port),
            device_type,
            client: reqwest::Client::new(),

            conn_cancel: None,

            cache: Arc::new(DashMap::new()),
            updates: None,

            input_banks: None,
            output_banks: None,

            client_id: rng.gen::<u32>(),
        }
    }

    fn new_from_mdns(r: &Service) -> Result<Self, DiscoveryError> {
        let mtype = match r.txt().iter().find(|(k, _)| k.eq(&"motu.mdns.type")) {
            Some((_, v)) => std::str::from_utf8(&v)?,
            None => return Err(DiscoveryError::DeviceType),
        };

        let device_type: DeviceType = From::from(mtype);

        Ok(Self::new(
            r.name(),
            &format!(
                "{}.{}",
                r.host().as_ref().ok_or(DiscoveryError::NoHost)?,
                r.domain().as_ref().ok_or(DiscoveryError::NoDomain)?
            ),
            r.port(),
            device_type,
        ))
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn device_type(&self) -> DeviceType {
        self.device_type
    }

    pub fn hostname(&self) -> String {
        self.hostname.clone()
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn input_banks(&self) -> Result<Arc<DashMap<u32, ChannelBank>>, DeviceError> {
        Ok(self
            .input_banks
            .as_ref()
            .ok_or(DeviceError::ChannelBanksNotInitalized)?
            .clone())
    }

    pub fn output_banks(&self) -> Result<Arc<DashMap<u32, ChannelBank>>, DeviceError> {
        Ok(self
            .output_banks
            .as_ref()
            .ok_or(DeviceError::ChannelBanksNotInitalized)?
            .clone())
    }

    pub fn updates(
        &self,
    ) -> Result<tokio::sync::broadcast::Receiver<(String, Value)>, DeviceError> {
        match self.connected {
            true => Ok(self
                .updates
                .as_ref()
                .ok_or(DeviceError::NotConnected)?
                .subscribe()),
            false => Err(DeviceError::NotConnected),
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

        let (cached_tx, cached_rx) = tokio::sync::oneshot::channel();

        let (update_tx, _) = tokio::sync::broadcast::channel(64);
        self.updates = Some(update_tx.clone());

        // Start background long polling
        tokio::spawn(async move {
            // Initial cache pass
            let res = Self::poll(&c, &url, &mut etag, client_id, &cache, &update_tx).await;
            let _ = cached_tx.send(res);

            // Long polling
            loop {
                tokio::select! {
                    // poll
                    res = Self::poll(&c, &url, &mut etag, client_id, &cache, &update_tx) => {
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

        self.connected = true;

        // Build mappings once we ready
        match cached_rx.await? {
            Ok(_) => {
                self.input_banks = Some(Arc::new(extchannel::build("ibank", self.cache.clone())?));
                self.output_banks = Some(Arc::new(extchannel::build("obank", self.cache.clone())?));
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub fn get(&self) -> Arc<DashMap<String, Value>> {
        self.cache.clone()
    }

    /// Simple method to search for a key, basiclaly .contains() helper for the backing map
    pub fn find_key(&self, key: &str) -> Vec<(String, Value)> {
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
        updates: &tokio::sync::broadcast::Sender<(String, Value)>,
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
            c.insert(item.0.clone(), v.clone());
            let _ = updates.send((item.0, v));
        }

        Ok(())
    }

    pub async fn set(&self, r: Request) -> Result<(), DeviceError> {
        self.set_keys(&[(r.key.as_str(), r.val)]).await
    }

    pub async fn set_keys(&self, data: &[(&str, Value)]) -> Result<(), DeviceError> {
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

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Request {
    key: String,
    val: Value,
}

impl std::ops::Add for Request {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self {
            key: format!("{}/{}", self.key, other.key),
            val: other.val,
        }
    }
}

#[derive(Error, Debug)]
pub enum DiscoveryError {
    #[error(transparent)]
    ZeroconfError(#[from] async_zeroconf::ZeroconfError),
    #[error(transparent)]
    UTF8CastError(#[from] std::str::Utf8Error),
    #[error("no host discovered?")]
    NoHost,
    #[error("no domain discovered?")]
    NoDomain,
    #[error("could not determine device type")]
    DeviceType,
    #[error("no device with name: `{0}` discovered")]
    NoDeviceWithNameDiscovered(String),
    #[error("no motu devices discovered")]
    NoDevice,
}

#[derive(Error, Debug)]
pub enum DeviceError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error(transparent)]
    SerializationError(#[from] serde_json::Error),
    #[error("could not connect to device: `{0}`")]
    CouldNotConnect(String),
    #[error("no connected to device yet, run connect?")]
    NotConnected,
    #[error(transparent)]
    ValueParsingError(#[from] ValueError),
    #[error(transparent)]
    DefinitionParsingError(#[from] ParseError),
    #[error("unexpected response from device: `{0}`: `{1}`")]
    BadResponse(StatusCode, String),
    #[error(transparent)]
    OneShotRecvError(#[from] tokio::sync::oneshot::error::RecvError),
    #[error("channel banks have not been built yet, did you run connect?")]
    ChannelBanksNotInitalized,
}
