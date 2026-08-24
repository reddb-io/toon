use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    #[default]
    Null,
    String(String),
    Number(i64),
}

pub type Value = JsonValue;

pub type Method = String;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Params {
    ByPosition(Vec<Value>),
    ByName(serde_json::Map<String, Value>),
    #[serde(skip)]
    #[default]
    Absent,
}

impl Params {
    pub fn is_empty(&self) -> bool {
        match self {
            Params::ByPosition(v) => v.is_empty(),
            Params::ByName(m) => m.is_empty(),
            Params::Absent => true,
        }
    }

    pub fn is_absent(&self) -> bool {
        matches!(self, Params::Absent)
    }
}
