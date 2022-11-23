use crate::value::{Value, ValueError};
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use uriparse::Segment;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BankType {
    Input,
    Output,
}

impl Default for BankType {
    fn default() -> Self {
        Self::Input
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Bank {
    pub name: Option<String>,
    pub t: BankType,
    pub smux: Option<String>,

    /// The number of channels available in this bank at its current sample rate.
    pub num_channels: u32,
    /// The maximum possible number of channels in the input or output bank.
    pub max_channels: u32,
    /// The number of channels that the user has enabled for this bank.
    pub user_channels: u32,
    /// The number of channels that are actually active. This is always the minimum of
    /// ext/<ibank_or_obank>/<index>/userCh and ext/<ibank_or_obank>/<index>/userCh.
    pub currenty_active_channels: u32,

    pub channels: HashMap<u32, ExtChannel>,
}

impl Bank {
    pub fn update(&mut self, key: &[Segment], value: &Value) -> Result<(), ParseError> {
        match key[0].as_str() {
            "name" => self.name = Some(value.to_string()),
            "numCh" => self.num_channels = value.try_into()?,
            "maxCh" => self.max_channels = value.try_into()?,
            "userCh" => self.user_channels = value.try_into()?,
            "calcCh" => self.currenty_active_channels = value.try_into()?,
            "smux" => self.smux = Some(value.to_string()),
            "ch" => {
                if key.len() >= 3 {
                    let ch = self
                        .channels
                        .entry(key[1].parse::<u32>()?)
                        .or_insert(ExtChannel::default());
                    ch.update(&key[2..], value)?;
                } else {
                    return Err(ParseError::NotEnoughDataInSegment);
                };
            }
            _ => {}
        };

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ExtChannel {
    default_name: Option<String>,
    name: Option<String>,
    src: Option<String>,

    trim: Option<i32>,
    trim_range: Option<(i32, i32)>,
    pad: Option<bool>,
    phase: Option<bool>,
    phantom_power: Option<bool>,
    connection: Option<bool>,
}

impl ExtChannel {
    pub fn update(&mut self, key: &[Segment], value: &Value) -> Result<(), ParseError> {
        match key[0].as_str() {
            "defaultName" => self.default_name = value.into(),
            "name" => self.name = value.into(),
            "src" => self.src = value.into(),
            "trim" => self.trim = Some(value.try_into()?),
            "trimRange" => self.trim_range = Some(value.try_into()?),
            "pad" => self.pad = Some(value.try_into()?),
            "phase" => self.phase = Some(value.try_into()?),
            "48V" => self.phase = Some(value.try_into()?),
            "connection" => self.phase = Some(value.try_into()?),
            _ => {}
        }
        Ok(())
    }
}

pub fn seed(cache: Arc<DashMap<String, Value>>) -> Result<(), ParseError> {
    let mut ibank: HashMap<u32, Bank> = HashMap::new();

    for item in cache.iter() {
        let k: &str = item.key();
        let m = uriparse::URIReference::try_from(k)?;
        let k = m.path().segments();

        let value = item.value();

        if k.len() > 2 && k[1] == "ibank" {
            let index = k[2].parse::<u32>()?;

            let b = ibank.entry(index).or_insert(Bank {
                t: BankType::Input,
                ..Default::default()
            });

            //let v = k[3..];

            b.update(&k[3..], value)?;
            //ibank.contains_key(2)
        }
    }

    dbg!(ibank);

    Ok(())
}

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("could not parse int")]
    UnableToParseInt,

    #[error("not enough data in segment")]
    NotEnoughDataInSegment,

    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),
    #[error(transparent)]
    ValueError(#[from] ValueError),
    #[error(transparent)]
    URIParseError(#[from] uriparse::URIReferenceError),
}
