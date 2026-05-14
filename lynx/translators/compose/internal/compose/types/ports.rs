use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StringOrU16 {
    String(String),
    Number(u16),
}

impl StringOrU16 {
    pub fn as_str_val(&self) -> String {
        match self {
            StringOrU16::String(s) => s.clone(),
            StringOrU16::Number(n) => n.to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PortMapping {
    Short(String),
    Long {
        target: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        published: Option<StringOrU16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        protocol: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        host_ip: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app_protocol: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}
