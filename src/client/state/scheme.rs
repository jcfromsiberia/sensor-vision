use regex::Regex;

use strum::{EnumIter, EnumProperty};

use crate::model::{MetricId, MqttId, SensorId};

#[derive(Clone, Copy, Debug, EnumIter, EnumProperty)]
pub enum MqttScheme {
    #[strum(props(
        path = "sensor/list",
        response = "inventory/inbox",
        error = "inventory/error/inbox"
    ))]
    SensorList,

    #[strum(props(
        path = "sensor/create",
        response = "sensor/inbox",
        error = "sensor/error/inbox"
    ))]
    SensorCreate,

    #[strum(props(
        path = "sensor/:mqttid:/update",
        response = "sensor/:mqttid:/update/info/inbox",
        error = "sensor/:mqttid:/update/error/inbox"
    ))]
    SensorUpdate(SensorId),

    #[strum(props(
        path = "sensor/:mqttid:/delete",
        response = "sensor/:mqttid:/delete/info/inbox",
        error = "sensor/:mqttid:/delete/error/inbox"
    ))]
    SensorDelete(SensorId),

    #[strum(props(
        path = "sensor/:mqttid:/metric/:mqttid:/inventory",
        response = "sensor/:mqttid:/metric/:mqttid:/inventory/inbox",
        error = "sensor/:mqttid:/metric/:mqttid:/inventory/error/inbox"
    ))]
    MetricDescribe(SensorId, MetricId),

    #[strum(props(
        path = "sensor/:mqttid:/metric/create",
        response = "sensor/:mqttid:/metric/inbox",
        error = "sensor/:mqttid:/metric/error/inbox"
    ))]
    MetricCreate(SensorId),

    #[strum(props(
        path = "sensor/:mqttid:/metric/update",
        response = "sensor/:mqttid:/metric/update/info/inbox",
        error = "sensor/:mqttid:/metric/update/error/inbox"
    ))]
    MetricUpdate(SensorId),

    #[strum(props(
        path = "sensor/:mqttid:/metric/delete",
        response = "sensor/:mqttid:/metric/delete/info/inbox",
        error = "sensor/:mqttid:/metric/delete/error/inbox"
    ))]
    MetricDelete(SensorId),

    #[strum(props(
        path = "sensor/:mqttid:/metric/pushValues",
        response = "sensor/:mqttid:/info/inbox",
        error = "sensor/:mqttid:/error/inbox"
    ))]
    PushValues(SensorId),

    #[strum(props(
        path = "ping",
        response = "ping/info/inbox",
        error = "ping/error/inbox"
    ))]
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
        (
            self.get_str("path").unwrap(),
            self.get_str("response").unwrap(),
            self.get_str("error").unwrap(),
        )
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
