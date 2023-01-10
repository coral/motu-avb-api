use crate::extchannel::{self, ChannelBank, ChannelBankType, ParseError};
use crate::value::{Value, ValueError};
use dashmap::DashMap;
use rand::Rng;
use reqwest::{header::HeaderValue, StatusCode};
use serde::ser::{Serialize as SerializeImpl, SerializeStruct};
use serde::{Deserialize, Serialize};
use serde_json::Value as SerdeValue;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::{collections::HashMap, fmt::Display};
use thiserror::Error;
use tokio::sync::mpsc::{channel, Sender};

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Hash)]
struct ShadowDevice {
    name: String,
    hostname: String,
    port: u16,
    uid: String,
    device_type: DeviceType,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Device {
    name: String,
    hostname: String,
    port: u16,
    uid: String,
    device_type: DeviceType,

    connected: bool,

    url: String,
    health: String,
    client: reqwest::Client,

    conn_cancel: Option<Sender<()>>,

    cache: Arc<DashMap<String, Value>>,
    updates: Option<tokio::sync::broadcast::Sender<Update>>,

    input_banks: Option<Arc<DashMap<u32, ChannelBank>>>,
    output_banks: Option<Arc<DashMap<u32, ChannelBank>>>,

    client_id: u32,
}

impl SerializeImpl for Device {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("Device", 5)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("hostname", &self.hostname)?;
        state.serialize_field("port", &self.port)?;
        state.serialize_field("uid", &self.uid)?;
        state.serialize_field("device_type", &self.device_type)?;
        state.end()
    }
}

impl From<ShadowDevice> for Device {
    fn from(v: ShadowDevice) -> Self {
        Device::new(&v.name, &v.hostname, v.port, &v.uid, v.device_type)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
pub enum DeviceType {
    Host,
    Device,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Update {
    Internal(String, Value),
    External(String, Value),
}

#[allow(dead_code)]
enum KeyType {
    InputBank(u32),
    OutputBank(u32),
    Mixer,
    AVB,
    NotImplemented,
}

impl KeyType {
    fn convert_from_str(key: &str) -> Result<(Self, uriparse::URIReference), DeviceError> {
        let m = uriparse::URIReference::try_from(key)?;
        let k = m.path().segments();
        if k.len() > 2 {
            let k = match k[0].as_str() {
                "ext" => {
                    let index = k[2].parse::<u32>()?;
                    match ChannelBankType::try_from(k[1].as_str())? {
                        ChannelBankType::Input => KeyType::InputBank(index),
                        ChannelBankType::Output => KeyType::OutputBank(index),
                    }
                }
                "avb" => KeyType::AVB,
                "mix" => KeyType::Mixer,
                _ => KeyType::NotImplemented,
            };

            return Ok((k, m));
        };

        Err(DeviceError::KeyParseError)
    }
}

impl Update {
    pub fn any(self) -> (String, Value) {
        match self {
            Update::Internal(k, v) => (k, v),
            Update::External(k, v) => (k, v),
        }
    }
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

impl Eq for Device {}

impl Hash for Device {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hostname.hash(state);
        self.port.hash(state);
        self.uid.hash(state);
    }
}

impl Device {
    pub fn new(
        name: &str,
        hostname: &str,
        port: u16,
        uid: &str,
        device_type: DeviceType,
    ) -> Device {
        let mut rng = rand::thread_rng();

        Device {
            name: name.to_string(),
            hostname: hostname.to_string(),
            port,
            uid: uid.to_string(),

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

    pub fn from_json(json_data: &str) -> Result<Device, DeviceError> {
        let shd: ShadowDevice = serde_json::from_str(json_data)?;
        Ok(Device::from(shd))
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

    pub fn updates(&self) -> Result<tokio::sync::broadcast::Receiver<Update>, DeviceError> {
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

        let (update_tx, mut map_update) = tokio::sync::broadcast::channel(64);
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
            }
            Err(e) => return Err(e),
        };

        let update_input_bank = self.input_banks.clone().unwrap();
        let update_output_bank = self.input_banks.clone().unwrap();

        // Listen to updates and map that to our internal representations
        tokio::spawn(async move {
            loop {
                let upd = match map_update.recv().await {
                    Ok(v) => v,
                    Err(e) => match e {
                        tokio::sync::broadcast::error::RecvError::Closed => return,
                        tokio::sync::broadcast::error::RecvError::Lagged(_) => continue,
                    },
                };

                let (k, value) = upd.any();

                if let Ok((tk, uri)) = KeyType::convert_from_str(k.as_str()) {
                    let k = uri.path().segments();
                    match tk {
                        KeyType::InputBank(index) => {
                            match update_input_bank.get_mut(&index) {
                                Some(mut v) => {
                                    v.update(&k[3..], &value).unwrap();
                                }
                                None => {}
                            };
                        }
                        KeyType::OutputBank(index) => {
                            match update_output_bank.get_mut(&index) {
                                Some(mut v) => {
                                    v.update(&k[3..], &value).unwrap();
                                }
                                None => {}
                            };
                        }
                        _ => {}
                    }
                }
            }
        });

        Ok(())
    }

    pub fn get(&self) -> Arc<DashMap<String, Value>> {
        self.cache.clone()
    }

    //fn mapped_updates() {}

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
        updates: &tokio::sync::broadcast::Sender<Update>,
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
            let _ = updates.send(Update::External(item.0, v));
        }

        Ok(())
    }

    pub async fn set(&self, r: crate::Request) -> Result<(), DeviceError> {
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
                // Update our internal cache
                for (key, val) in data.into_iter() {
                    self.cache.insert(key.to_string(), val.clone());
                    if let Some(upd) = &self.updates {
                        upd.send(Update::Internal(key.to_string(), val.clone()))?;
                    }
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

    pub fn uid(&self) -> &str {
        //self.get_value("uid").map(Into::into)
        &self.uid
    }
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
    #[error("could not parse key, not big")]
    KeyParseError,
    #[error(transparent)]
    BroadcastError(#[from] tokio::sync::broadcast::error::SendError<Update>),
    #[error(transparent)]
    URIParseError(#[from] uriparse::URIReferenceError),
    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),
}
