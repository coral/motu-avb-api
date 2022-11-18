use regex::Regex;
use serde_json::{json, Value as SerdeValue};
use thiserror::Error;

lazy_static! {
    static ref BOOL_MATCHER: Regex = Regex::new(r"(?i)(bool)").unwrap();
    static ref PAIR_MATCHER: Regex = Regex::new(r"(?i)(name)").unwrap();
}

#[derive(Clone, Debug)]
pub struct MEnum {
    pub value: u32,
    pub definitions: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum Value {
    String(String),
    Float(f64),
    Int(i64),
    Semver(String),
    Bool(bool),
    Enum(MEnum),
    Pair(Vec<String>),
}

impl Value {
    pub fn decode(self, key: &str) -> Value {
        // Currently only have to deal with strings lol
        // Why did MOTU have to reinvent JSON?
        // I hate this.
        let s = match self {
            Value::String(v) => v,
            _ => return self,
        };

        if BOOL_MATCHER.is_match(key) {
            return self;
        }

        if PAIR_MATCHER.is_match(key) {
            return self;
        }

        Value::Bool(true)
    }
}

impl TryFrom<SerdeValue> for Value {
    type Error = ValueError;

    fn try_from(val: SerdeValue) -> Result<Self, Self::Error> {
        match val {
            SerdeValue::Null => Err(ValueError::NoValue),
            SerdeValue::Bool(v) => Ok(Value::Bool(v)),
            SerdeValue::Number(v) if v.is_f64() => Ok(Value::Float(
                v.as_f64().ok_or(ValueError::UnableToParseFloat)?,
            )),
            SerdeValue::Number(v) if v.is_i64() => {
                Ok(Value::Int(v.as_i64().ok_or(ValueError::UnableToParseInt)?))
            }
            SerdeValue::Number(v) if v.is_u64() => Ok(Value::Int(
                v.as_u64().ok_or(ValueError::UnableToParseInt)? as i64,
            )),
            SerdeValue::Number(v) => Err(ValueError::UnableToParseInt),
            SerdeValue::String(v) => Ok(Value::String(v)),
            SerdeValue::Array(_) => Err(ValueError::WTF),
            SerdeValue::Object(_) => Err(ValueError::WTF),
        }
    }
}

#[derive(Error, Debug)]
pub enum ValueError {
    #[error("could not parse int")]
    UnableToParseInt,
    #[error("could not parse float")]
    UnableToParseFloat,
    #[error("not able to decode value")]
    NoValue,
    #[error("this value should not exist")]
    WTF,
}
