use std::fmt::Formatter;
use std::convert::{From, Into};
use uuid::Uuid;
use serde::{Deserialize, Serialize};

pub mod protocol;
pub mod sensor;

#[derive(Copy, Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MqttId {
    #[serde(with = "uuid::serde::simple")]
    uuid: Uuid,
}

impl MqttId {
    pub fn is_nil(&self) -> bool {
        self.uuid.is_nil()
    }
}

impl std::fmt::Display for MqttId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", <&MqttId as Into<String>>::into(self))
    }
}

impl From<MqttId> for String {
    fn from(value: MqttId) -> Self {
        if value.uuid.is_nil() {
            "+".to_owned()
        } else {
            value.uuid.as_simple().to_string()
        }
    }
}

impl From<&MqttId> for String {
    fn from(value: &MqttId) -> Self {
        Self::from(*value)
    }
}

impl From<String> for MqttId {
    fn from(value: String) -> Self {
        value.as_str().into()
    }
}

impl Into<MqttId> for &str {
    fn into(self) -> MqttId {
        MqttId {
            uuid: Uuid::parse_str(self).expect("Failed to parse uuid")
        }
    }
}

impl From<Uuid> for MqttId {
    fn from(value: Uuid) -> Self {
        Self {uuid: value}
    }
}

// TODO apply strong typedef
pub type ConnectorId = MqttId;
pub type SensorId = MqttId;
pub type MetricId = MqttId;
