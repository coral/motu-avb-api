use regex::Regex;
use serde::ser::{Serialize, Serializer};
use serde_json::Value as SerdeValue;
use thiserror::Error;

lazy_static! {
    static ref BOOL_MATCHER: Regex = Regex::new(r"bool:(.*)").unwrap();
    static ref NAME_ESCAPE_MATCHER: Regex = Regex::new(r"(?i)(name)").unwrap();
}

#[derive(Clone, Debug)]
pub struct MEnum {
    pub value: i64,
    pub definitions: Vec<(i64, String)>,
}

#[derive(Clone, Debug)]
pub enum Value {
    String(String),
    Float(f64),
    Int(i64),
    Bool(bool),
    Enum(MEnum),
    Pair(Vec<String>),
}

impl Value {
    pub fn decode(self, key: &str) -> Result<Value, ValueError> {
        // Currently only have to deal with strings lol
        // Why did MOTU have to reinvent JSON?
        // I hate this.
        let s = match &self {
            Value::String(v) => v,
            _ => return Ok(self),
        };

        if s.len() == 0 {
            return Ok(self);
        }

        // MOTU uses : to deliminate different variables but a name with : is valid so
        // we need to escape : if the key contains :
        // So dumb
        if NAME_ESCAPE_MATCHER.is_match(key) {
            return Ok(self);
        }

        // match weird bool
        match BOOL_MATCHER.captures(key) {
            Some(v) => {
                let val = match &v[0] {
                    "1" => true,
                    "0" => false,
                    _ => false,
                };

                return Ok(Value::Bool(val));
            }
            None => {}
        }

        let spliced: Vec<&str> = s.split(":").collect();

        // Check for matches
        if spliced.len() == 0 {
            return Ok(self);
        }

        // Lets find out if this is an enum

        if spliced[0] == "enum" {
            let value = spliced[1].parse::<i64>()?;
            let definitions: Vec<(i64, String)> = spliced
                .iter()
                .skip(2)
                .map(|f| {
                    let vm: Vec<&str> = f.split("=").collect();
                    (vm[0].parse::<i64>().unwrap(), vm[1].to_string())
                })
                .collect();

            return Ok(Value::Enum(MEnum { value, definitions }));
        }

        // TODO implement real
        // TODO implement all the other weird shit MOTU stuffed into their key value store

        // If not, it's a pair
        Ok(Value::Pair(spliced.iter().map(|f| f.to_string()).collect()))
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
            SerdeValue::Number(_) => Err(ValueError::UnableToParseInt),
            SerdeValue::String(v) => Ok(Value::String(v)),
            SerdeValue::Array(_) => Err(ValueError::WTF),
            SerdeValue::Object(_) => Err(ValueError::WTF),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", &String::from(self))
    }
}

impl From<Value> for String {
    fn from(val: Value) -> String {
        match val {
            Value::String(v) => v.clone(),
            Value::Float(v) => v.to_string(),
            Value::Int(v) => v.to_string(),
            Value::Bool(v) => v.to_string(),
            Value::Enum(v) => v.value.to_string(),
            Value::Pair(v) => v.join(":"),
        }
    }
}

impl From<&Value> for String {
    fn from(val: &Value) -> String {
        match val {
            Value::String(v) => v.clone(),
            Value::Float(v) => v.to_string(),
            Value::Int(v) => v.to_string(),
            Value::Bool(v) => v.to_string(),
            Value::Enum(v) => v.value.to_string(),
            Value::Pair(v) => v.join(":"),
        }
    }
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Value::String(v) => serializer.serialize_str(v),
            Value::Float(v) => serializer.serialize_f64(*v),
            Value::Int(v) => serializer.serialize_i64(*v),
            Value::Bool(v) => {
                let value = match v {
                    true => "1",
                    false => "0",
                };
                serializer.serialize_str(&format!("bool:{}", value))
            }
            Value::Enum(v) => serializer.serialize_i64(v.value),
            Value::Pair(_) => serializer.serialize_str(&String::from(self)),
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
    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),
}
