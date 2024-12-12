use uuid::Uuid;

pub mod protocol;
pub mod sensor;

pub trait ToMqttId {
    fn to_mqtt(&self) -> String;
}

impl ToMqttId for Uuid {
    fn to_mqtt(&self) -> String {
        if self.is_nil() {
            "+".to_owned()
        } else {
            self.as_simple().to_string()
        }
    }
}