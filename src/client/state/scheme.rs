use regex::Regex;

use strum::EnumIter;

use crate::model::{MetricId, MqttId, SensorId};

#[derive(Clone, Copy, Debug, EnumIter)]
pub enum MqttScheme {
    SensorList,
    SensorCreate,
    SensorUpdate(SensorId),
    SensorDelete(SensorId),
    MetricDescribe(SensorId, MetricId),
    MetricCreate(SensorId),
    MetricUpdate(SensorId),
    MetricDelete(SensorId),
    PushValues(SensorId),
    Ping,
}

macro_rules! to_mqtt_topic {
    ($format_str:expr, $($args:expr),* $(,)?) => {
        {
            let args: &[String] = &[$($args.into()),*];
            MqttScheme::render_topic($format_str, args)
        }
    };
}

impl MqttScheme {
    pub fn get_templates(&self) -> (&'static str, &'static str, &'static str) {
        use MqttScheme::*;
        match self {
            SensorList => ("sensor/list", "inventory/inbox", "inventory/error/inbox"),
            SensorCreate => ("sensor/create", "sensor/inbox", "sensor/error/inbox"),
            SensorUpdate(..) => (
                "sensor/:mqttid:/update",
                "sensor/:mqttid:/update/info/inbox",
                "sensor/:mqttid:/update/error/inbox",
            ),
            SensorDelete(..) => (
                "sensor/:mqttid:/delete",
                "sensor/:mqttid:/delete/info/inbox",
                "sensor/:mqttid:/delete/error/inbox",
            ),
            MetricDescribe(..) => (
                "sensor/:mqttid:/metric/:mqttid:/inventory",
                "sensor/:mqttid:/metric/:mqttid:/inventory/inbox",
                "sensor/:mqttid:/metric/:mqttid:/inventory/error/inbox",
            ),
            MetricCreate(..) => (
                "sensor/:mqttid:/metric/create",
                "sensor/:mqttid:/metric/inbox",
                "sensor/:mqttid:/metric/error/inbox",
            ),
            MetricUpdate(..) => (
                "sensor/:mqttid:/metric/update",
                "sensor/:mqttid:/metric/update/info/inbox",
                "sensor/:mqttid:/metric/update/error/inbox",
            ),
            MetricDelete(..) => (
                "sensor/:mqttid:/metric/delete",
                "sensor/:mqttid:/metric/delete/info/inbox",
                "sensor/:mqttid:/metric/error/inbox",
            ),
            PushValues(..) => (
                "sensor/:mqttid:/metric/pushValues",
                "sensor/:mqttid:/info/inbox",
                "sensor/:mqttid:/error/inbox",
            ),
            Ping => ("ping", "ping/info/inbox", "ping/error/inbox"),
        }
    }

    pub fn get_topics(&self) -> (String, String, String) {
        use MqttScheme::*;
        let (request, response, error) = self.get_templates();
        match self {
            SensorUpdate(sensor_id)
            | SensorDelete(sensor_id)
            | MetricCreate(sensor_id)
            | MetricUpdate(sensor_id)
            | MetricDelete(sensor_id)
            | PushValues(sensor_id) => (
                to_mqtt_topic!(request, sensor_id),
                to_mqtt_topic!(response, sensor_id),
                to_mqtt_topic!(error, sensor_id),
            ),
            MetricDescribe(sensor_id, metric_id) => (
                to_mqtt_topic!(request, sensor_id, metric_id),
                to_mqtt_topic!(response, sensor_id, metric_id),
                to_mqtt_topic!(error, sensor_id, metric_id),
            ),
            _ => (request.to_owned(), response.to_owned(), error.to_owned()),
        }
    }

    pub fn render_topic(template: &str, args: &[String]) -> String {
        let mut result = template.to_string();
        for arg in args {
            result = result.replacen(":mqttid:", arg, 1);
        }
        result
    }

    pub fn extract_ids_and_pattern(topic: &str) -> (Vec<MqttId>, String) {
        let re = Regex::new(r"/([a-f0-9]{32})/").expect("Failed to create regex");
        let mqtt_ids = re
            .captures_iter(topic)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .map(|m| m.as_str().into())
            .collect();
        let pattern = re.replace_all(topic, "/:mqttid:/").to_string();
        (mqtt_ids, pattern)
    }
}
