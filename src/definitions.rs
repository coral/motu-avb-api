use crate::value::{Value, ValueError};
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use uriparse::Segment;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelBankType {
    Input,
    Output,
}

impl Default for ChannelBankType {
    fn default() -> Self {
        Self::Input
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ChannelBank {
    /// The name of the input or output ChannelBank
    pub name: Option<String>,
    /// Input or Output
    pub t: ChannelBankType,
    /// Manual says: `For Optical ChannelBanks, either "toslink" or "adat"`
    /// This however is a lie because the soundcard returns "standard"
    /// i don't even...
    pub smux: Option<String>,
    /// The number of channels available in this ChannelBank at its current sample rate.
    pub num_channels: u32,
    /// The maximum possible number of channels in the input or output ChannelBank.
    pub max_channels: u32,
    /// The number of channels that the user has enabled for this ChannelBank.
    pub user_channels: u32,
    /// The number of channels that are actually active. This is always the minimum of
    /// ext/<iChannelBank_or_oChannelBank>/<index>/userCh and ext/<iChannelBank_or_oChannelBank>/<index>/userCh.
    pub currenty_active_channels: u32,

    /// Map of all the channels for the ChannelBank
    pub channels: HashMap<u32, ExtChannel>,
}

impl ChannelBank {
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
    /// Default name of the channel, not even documented in the manual lol
    pub default_name: Option<String>,
    /// User set name
    pub name: Option<String>,
    /// If the output channel is connected to an input ChannelBank, a ":" separated pair in the form ":"
    /// otherwise, if unrouted, None
    pub src: Option<String>,
    /// Defines trim properties if they are available for the channel
    pub trim: Option<Trim>,
    pub pad: Option<bool>,
    /// True if the signal has its phase inverted. This is only applicable to some input or output channels.
    pub phase: Option<bool>,
    /// True if the 48V phantom power is engaged. This is only applicable to some input channels.
    pub phantom_power: Option<bool>,
    /// True if the channel has a physical connector plugged in (e.g., an audio jack). This information may not be
    /// available for all ChannelBanks or devices.
    pub connection: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TrimValue {
    /// A dB-value for how much to trim this input or output channel. The range of this parameter is indicated by trim_range
    pub trim: i32,
    /// Pair describing the trim range for the channel
    pub trim_range: (i32, i32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Trim {
    Mono(TrimValue),
    Stereo(TrimValue),
}

impl ExtChannel {
    pub fn update(&mut self, key: &[Segment], value: &Value) -> Result<(), ParseError> {
        match key[0].as_str() {
            "defaultName" => self.default_name = value.into(),
            "name" => self.name = value.into(),
            "src" => self.src = value.into(),
            "trim" => self.set_mono_trim(value.try_into()?),
            "trimRange" => self.set_mono_trim_range(value.try_into()?),
            "stereoTrim" => self.set_stereo_trim(value.try_into()?),
            "stereoTrimRange" => self.set_stereo_trim_range(value.try_into()?),
            "pad" => self.pad = Some(value.try_into()?),
            "phase" => self.phase = Some(value.try_into()?),
            "48V" => self.phase = Some(value.try_into()?),
            "connection" => self.phase = Some(value.try_into()?),
            _ => {}
        }
        Ok(())
    }

    // There must be a better way of doing this
    // I'm just not good enough at rust yet..
    fn set_mono_trim(&mut self, val: i32) {
        if let Trim::Mono(t) = self.trim.get_or_insert(Trim::Mono(TrimValue::default())) {
            t.trim = val;
        }
    }
    fn set_mono_trim_range(&mut self, val: (i32, i32)) {
        if let Trim::Mono(t) = self.trim.get_or_insert(Trim::Mono(TrimValue::default())) {
            t.trim_range = val;
        }
    }
    fn set_stereo_trim(&mut self, val: i32) {
        if let Trim::Stereo(t) = self.trim.get_or_insert(Trim::Mono(TrimValue::default())) {
            t.trim = val;
        }
    }
    fn set_stereo_trim_range(&mut self, val: (i32, i32)) {
        if let Trim::Stereo(t) = self.trim.get_or_insert(Trim::Mono(TrimValue::default())) {
            t.trim_range = val;
        }
    }
}

pub fn build(
    prefix: &str,
    cache: Arc<DashMap<String, Value>>,
) -> Result<HashMap<u32, ChannelBank>, ParseError> {
    let mut channel_bank: HashMap<u32, ChannelBank> = HashMap::new();

    for item in cache.iter() {
        let m = uriparse::URIReference::try_from(item.key() as &str)?;
        let k = m.path().segments();

        let value = item.value();

        if k.len() > 2 && k[1] == prefix {
            let index = k[2].parse::<u32>()?;

            let b = channel_bank.entry(index).or_insert(ChannelBank {
                t: ChannelBankType::Input,
                ..Default::default()
            });

            b.update(&k[3..], value)?;
        }
    }

    Ok(channel_bank)
}

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("could not parse int")]
    UnableToParseInt,

    #[error("not enough data in segment")]
    NotEnoughDataInSegment,

    #[error("wtf")]
    WTF,

    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),
    #[error(transparent)]
    ValueError(#[from] ValueError),
    #[error(transparent)]
    URIParseError(#[from] uriparse::URIReferenceError),
}
