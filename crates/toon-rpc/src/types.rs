use serde_json::Value as JsonValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Id {
    Null,
    String(String),
    Number(i64),
}

impl Default for Id {
    fn default() -> Self {
        Id::Null
    }
}

impl Id {
    pub fn is_notification(&self) -> bool {
        matches!(self, Id::Null)
    }
}

pub type Value = JsonValue;

pub type Method = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Params {
    ByPosition(Vec<Value>),
    ByName(serde_json::Map<String, Value>),
}

impl Params {
    pub fn is_empty(&self) -> bool {
        match self {
            Params::ByPosition(v) => v.is_empty(),
            Params::ByName(m) => m.is_empty(),
        }
    }
}

impl Default for Params {
    fn default() -> Self {
        Params::ByPosition(vec![])
    }
}
