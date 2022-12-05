use crate::device::Device;
use async_zeroconf::Service;
use regex::Regex;
use std::collections::HashSet;
use std::time::Duration;
use thiserror::Error;

lazy_static! {
    static ref BOOL_MATCHER: Regex = Regex::new(r"MOTU Pro Audio HTTP Host: (.*)").unwrap();
}

#[allow(dead_code)]
pub async fn from_name(name: &str, timeout: Option<Duration>) -> Result<Device, DiscoveryError> {
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

            return new_from_mdns(&resolved_service);
        }
    }

    Err(DiscoveryError::NoDeviceWithNameDiscovered(name.to_string()))
}

#[allow(dead_code)]
pub async fn discover(timeout: Option<Duration>) -> Result<Vec<Device>, DiscoveryError> {
    // Default duration of 10 secs
    let timeout = match timeout {
        Some(v) => v,
        None => Duration::from_secs(10),
    };

    let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
    let mut services = browser.timeout(timeout).browse()?;

    let mut devices = Vec::new();
    let mut found: HashSet<String> = HashSet::new();

    while let Some(Ok(v)) = services.recv().await {
        let resolved_service = async_zeroconf::ServiceResolver::r(&v).await?;

        match resolved_service
            .txt()
            .iter()
            .find(|(k, _)| k.contains("motu.mdns.type"))
        {
            Some((_, v)) => {
                let d = std::str::from_utf8(v)?;
                if d.contains("netiodevice") {
                    let nd = new_from_mdns(&resolved_service)?;
                    if !found.contains(nd.uid()) {
                        found.insert(nd.uid().to_string());
                        devices.push(nd.clone());
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

#[allow(dead_code)]
pub async fn streaming_discover(
    timeout: Option<Duration>,
) -> Result<tokio::sync::mpsc::Receiver<Result<Device, DiscoveryError>>, DiscoveryError> {
    let (tx, rx) = tokio::sync::mpsc::channel(10);

    // Default duration of 10 secs
    let timeout = match timeout {
        Some(v) => v,
        None => Duration::from_secs(20),
    };

    let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
    let mut services = browser.timeout(timeout).browse()?;

    let mut devices = Vec::new();

    tokio::spawn(async move {
        while let Some(Ok(v)) = services.recv().await {
            let resolved_service = match async_zeroconf::ServiceResolver::r(&v).await {
                Ok(v) => v,
                Err(e) => {
                    tx.send(Err(DiscoveryError::ZeroconfError(e)))
                        .await
                        .unwrap();
                    continue;
                }
            };

            match resolved_service
                .txt()
                .iter()
                .find(|(k, _)| k.contains("motu.mdns.type"))
            {
                Some((_, v)) => {
                    let d = match std::str::from_utf8(v) {
                        Ok(v) => v,
                        Err(e) => {
                            tx.send(Err(DiscoveryError::UTF8CastError(e)))
                                .await
                                .unwrap();
                            continue;
                        }
                    };

                    if d.contains("netiodevice") {
                        let nd = match new_from_mdns(&resolved_service) {
                            Ok(v) => v,
                            Err(e) => {
                                tx.send(Err(e)).await.unwrap();
                                continue;
                            }
                        };
                        if devices.iter().find(|v| **v == nd).is_none() {
                            devices.push(nd.clone());
                            tx.send(Ok(nd)).await.unwrap();
                        }
                    }
                }
                None => {}
            };
        }
    });

    Ok(rx)
}

fn new_from_mdns(r: &Service) -> Result<Device, DiscoveryError> {
    let mtype = match r.txt().iter().find(|(k, _)| k.eq(&"motu.mdns.type")) {
        Some((_, v)) => std::str::from_utf8(&v)?,
        None => return Err(DiscoveryError::DeviceType),
    };

    let device_type: crate::device::DeviceType = From::from(mtype);

    let uid = match device_type {
        crate::device::DeviceType::Host => {
            let cap = BOOL_MATCHER
                .captures(r.name())
                .ok_or(DiscoveryError::NoUIDForHost(r.name().to_string()))?;
            cap[1].to_string()
        }
        crate::device::DeviceType::Device => match r.txt().iter().find(|(k, _)| k.eq(&"uid")) {
            Some((_, v)) => std::str::from_utf8(&v)?.to_string(),
            None => return Err(DiscoveryError::DeviceType),
        },
        crate::device::DeviceType::Unknown => "UNKNOWN (this is a bug)".to_string(),
    };

    Ok(Device::new(
        r.name(),
        &format!(
            "{}.{}",
            r.host().as_ref().ok_or(DiscoveryError::NoHost)?,
            r.domain().as_ref().ok_or(DiscoveryError::NoDomain)?
        ),
        r.port(),
        &uid,
        device_type,
    ))
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
    #[error("could not decode uid for host `{0}")]
    NoUIDForHost(String),
    #[error("could not determine device type")]
    DeviceType,
    #[error("no device with name: `{0}` discovered")]
    NoDeviceWithNameDiscovered(String),
    #[error("no motu devices discovered")]
    NoDevice,
}
