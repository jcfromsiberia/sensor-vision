use eyre::{OptionExt, Result, WrapErr};

use regex::Regex;

use signals2::{Connect1, Emit1};

use std::collections::HashMap;
use std::sync::{Arc, RwLock, Weak};

use uuid::Uuid;

use crate::client::mqtt::MqttClientWrapper;
use crate::model::protocol::CreateMetricResponsePayload;
use crate::model::sensor::{LinkedMetric, Metric, Sensor};
use crate::model::ToMqttId;

pub enum MqttRequest<'a> {
    SensorList,
    SensorCreate,
    SensorUpdate(&'a Uuid),
    SensorDelete(&'a Uuid),
    MetricDescribe {
        sensor_id: &'a Uuid,
        metric_id: &'a Uuid,
    },
    MetricCreate(&'a Uuid),
    MetricUpdate(&'a Uuid),
    MetricDelete(&'a Uuid),
    PushValues(&'a Uuid),
    Ping,
}

impl MqttRequest<'_> {
    pub fn get_topics(&self) -> (String, String, String) {
        match self {
            MqttRequest::SensorList => (
                "sensor/list".to_owned(),
                "inventory/inbox".to_owned(),
                "inventory/error/inbox".to_owned(),
            ),
            MqttRequest::SensorCreate => (
                "sensor/create".to_owned(),
                "sensor/inbox".to_owned(),
                "sensor/error/inbox".to_owned(),
            ),
            MqttRequest::SensorUpdate(sensor_id) => (
                format!("sensor/{}/update", sensor_id.to_mqtt()),
                format!("sensor/{}/update/info/inbox", sensor_id.to_mqtt()),
                format!("sensor/{}/update/error/inbox", sensor_id.to_mqtt()),
            ),
            MqttRequest::SensorDelete(sensor_id) => (
                format!("sensor/{}/delete", sensor_id),
                format!("sensor/{}/delete/info/inbox", sensor_id.to_mqtt()),
                format!("sensor/{}/delete/error/inbox", sensor_id.to_mqtt()),
            ),
            MqttRequest::MetricDescribe {
                sensor_id,
                metric_id,
            } => (
                format!(
                    "sensor/{}/metric/{}/inventory",
                    sensor_id.to_mqtt(),
                    metric_id.to_mqtt()
                ),
                format!(
                    "sensor/{}/metric/{}/inventory/inbox",
                    sensor_id.to_mqtt(),
                    metric_id.to_mqtt()
                ),
                format!(
                    "sensor/{}/metric/{}/inventory/error/inbox",
                    sensor_id.to_mqtt(),
                    metric_id.to_mqtt()
                ),
            ),
            MqttRequest::MetricCreate(sensor_id) => (
                format!("sensor/{}/metric/create", sensor_id.to_mqtt()),
                format!("sensor/{}/metric/inbox", sensor_id.to_mqtt()),
                format!("sensor/{}/metric/error/inbox", sensor_id.to_mqtt()),
            ),
            MqttRequest::MetricUpdate(sensor_id) => (
                format!("sensor/{}/metric/update", sensor_id.to_mqtt()),
                format!("sensor/{}/metric/update/info/inbox", sensor_id.to_mqtt()),
                format!("sensor/{}/metric/update/error/inbox", sensor_id.to_mqtt()),
            ),
            MqttRequest::MetricDelete(sensor_id) => (
                format!("sensor/{}/metric/delete", sensor_id.to_mqtt()),
                format!("sensor/{}/metric/delete/info/inbox", sensor_id.to_mqtt()),
                format!("sensor/{}/metric/error/inbox", sensor_id.to_mqtt()),
            ),
            MqttRequest::PushValues(sensor_id) => (
                format!("sensor/{}/metric/pushValues", sensor_id.to_mqtt()),
                format!("sensor/{}/info/inbox", sensor_id.to_mqtt()),
                format!("sensor/{}/error/inbox", sensor_id.to_mqtt()),
            ),
            MqttRequest::Ping => (
                "ping".to_owned(),
                "ping/info/inbox".to_owned(),
                "ping/error/inbox".to_owned(),
            ),
        }
    }
}

enum MqttResponse {
    Ok { topic: String, message: String },
    Err { topic: String, message: String },
}

impl MqttResponse {
    pub fn get_ids(&self) -> Vec<Uuid> {
        let topic = self.get_topic();

        let re = Regex::new(r"/([a-f0-9]{32})/").expect("Failed to create regex");

        re.captures_iter(&topic)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .skip(1) // skip connector id
            .map(|m| Uuid::parse_str(m.as_str()).expect("Failed to parse uuid"))
            .collect()
    }

    pub fn get_topic(&self) -> &String {
        match self {
            MqttResponse::Ok { topic, .. } => topic,
            MqttResponse::Err { topic, .. } => topic,
        }
    }

    pub fn get_message(&self) -> &String {
        match self {
            MqttResponse::Ok { message, .. } => message,
            MqttResponse::Err { message, .. } => message,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SensorStateEvent {
    NewLinkedSensorLoaded(Sensor<LinkedMetric>),
    NewSensorCreated(Sensor<Metric>),
    NewMetricCreated { sensor_id: Uuid, metric_id: Uuid },
    NewMetricLoaded { sensor_id: Uuid, metric: Metric },
}

pub struct SensorState {
    pub sensors: HashMap<Uuid, Sensor<Metric>>,

    connector_id: Uuid,

    topic_callbacks: HashMap<
        String,
        Box<dyn Fn(&mut Self, String, String) -> Result<bool> + Send + Sync + 'static>,
    >,

    state_event: signals2::Signal<(SensorStateEvent,)>,

    events_mqtt_connection: Option<signals2::Connection>,
    weak_self: Weak<RwLock<SensorState>>,
}

impl SensorState {
    pub fn new(connector_id: Uuid) -> Arc<RwLock<Self>> {
        let mut result = Arc::new(RwLock::new(Self {
            connector_id,
            sensors: HashMap::new(),
            topic_callbacks: HashMap::new(),
            state_event: signals2::Signal::new(),
            events_mqtt_connection: None,
            weak_self: Weak::new(),
        }));

        result.write().unwrap().weak_self = Arc::downgrade(&result);

        result
    }

    pub fn subscribe_to_state_events<Slot>(
        &mut self,
        slot: Slot,
    ) -> Result<signals2::Connection>
    where
        Slot: Fn(SensorStateEvent) + Send + Sync + 'static,
    {
        Ok(self.state_event.connect(slot))
    }

    pub fn subscribe_to_mqtt_events(
        &mut self,
        mqtt_client: &mut MqttClientWrapper,
    ) -> Result<()> {
        let generic_topic = self.full_topic("#");

        let weak_self = self.weak_self.clone();
        let connection = mqtt_client.subscribe(&generic_topic, move |topic, message| {
            if let Some(this) = weak_self.upgrade() {
                this.write().unwrap().process_message(topic, message);
            }
        })?;

        self.events_mqtt_connection = Some(connection);

        self.assign_callback(&MqttRequest::SensorList, Self::cb_event_sensor_list);
        self.assign_callback(&MqttRequest::SensorCreate, Self::cb_event_sensor_create);

        Ok(())
    }

    fn process_message(&mut self, topic: String, message: String) {
        if let Some(callback) = self.topic_callbacks.get(&topic) {
            let callback: &Box<
                dyn Fn(&mut Self, String, String) -> Result<bool> + Send + Sync + 'static,
            > = callback;
            let callback_ptr: *const Box<
                dyn Fn(&mut Self, String, String) -> Result<bool> + Send + Sync + 'static,
            > = callback as *const _;
            let result = unsafe { (**callback_ptr)(self, topic, message) }; // Trust me, bro
            match result {
                Ok(changed) => {
                    if changed {
                        println!("!!!State changed!!!");
                    }
                }
                // TODO Signalize error
                Err(e) => eprintln!("{}", e),
            }
        }
    }

    fn full_topic(&self, topic: &str) -> String {
        format!("/v1.0/{}/{}", self.connector_id.as_simple(), topic)
    }

    fn assign_callback<Callback>(&mut self, action: &MqttRequest, callback: Callback)
    where
        Callback: Fn(&mut SensorState, MqttResponse) -> Result<bool>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let (_, response_topic, error_topic) = action.get_topics();

        {
            let cb_clone = callback.clone();
            let err_callback = move |this: &mut Self, topic: String, message: String| {
                cb_clone(this, MqttResponse::Err { topic, message })
            };
            self.topic_callbacks
                .insert(self.full_topic(&error_topic), Box::new(err_callback));
        }

        let ok_callback = move |this: &mut Self, topic: String, message: String| {
            callback(this, MqttResponse::Ok { topic, message })
        };
        self.topic_callbacks
            .insert(self.full_topic(&response_topic), Box::new(ok_callback));
    }

    fn subscribe_to_sensor_events(&mut self, sensor_id: &Uuid) {
        self.assign_callback(
            &MqttRequest::SensorUpdate(&sensor_id),
            Self::cb_event_sensor_update,
        );
        self.assign_callback(
            &MqttRequest::SensorUpdate(&sensor_id),
            Self::cb_event_sensor_update,
        );
        self.assign_callback(
            &MqttRequest::SensorDelete(&sensor_id),
            Self::cb_event_sensor_delete,
        );

        self.assign_callback(
            &MqttRequest::MetricCreate(&sensor_id),
            Self::cb_event_metric_create,
        );
        self.assign_callback(
            &MqttRequest::MetricUpdate(&sensor_id),
            Self::cb_event_metric_update,
        );
        self.assign_callback(
            &MqttRequest::MetricDelete(&sensor_id),
            Self::cb_event_metric_delete,
        );

        self.assign_callback(
            &MqttRequest::PushValues(&sensor_id),
            Self::cb_event_push_values,
        );

        // TODO Subscribe for livedata here
    }

    fn cb_event_sensor_list(&mut self, response: MqttResponse) -> Result<bool> {
        // TODO handle error case
        let message = response.get_message();

        let linked_sensors = serde_json::from_str::<Vec<Sensor<LinkedMetric>>>(&message)
            .wrap_err_with(|| format!("Failed to deserialize: {}", &message))?;

        let mut state_changed = false;
        // Merging new sensors
        for linked_sensor in linked_sensors {
            if self.sensors.get(&linked_sensor.sensor_id).is_none() {
                let event = SensorStateEvent::NewLinkedSensorLoaded(linked_sensor.clone());

                let sensor_id = linked_sensor.sensor_id.clone();
                let new_sensor = Sensor {
                    name: linked_sensor.name,
                    connector_id: linked_sensor.connector_id,
                    sensor_id: linked_sensor.sensor_id,
                    metrics: Vec::new(),
                };

                self.sensors.insert(sensor_id, new_sensor);

                self.subscribe_to_sensor_events(&sensor_id);

                for linked_metric in &linked_sensor.metrics {
                    self.assign_callback(
                        &MqttRequest::MetricDescribe {
                            sensor_id: &sensor_id,
                            metric_id: &linked_metric.metric_id,
                        },
                        Self::cb_event_metric_describe,
                    );
                }

                state_changed = true;
                self.state_event.emit(event);
            }
        }

        Ok(state_changed)
    }

    fn cb_event_sensor_create(&mut self, response: MqttResponse) -> Result<bool> {
        let message = response.get_message();

        let new_sensor = serde_json::from_str::<Sensor<Metric>>(&message)
            .wrap_err_with(|| format!("Failed to deserialize: {}", &message))?;

        let sensor_id = new_sensor.sensor_id.clone();

        self.subscribe_to_sensor_events(&sensor_id);

        // Should be no metrics in the new sensor => not subscribing to metric MetricDescribe

        self.sensors.insert(sensor_id, new_sensor.clone());

        self.state_event
            .emit(SensorStateEvent::NewSensorCreated(new_sensor));

        Ok(true)
    }

    fn cb_event_sensor_update(&mut self, response: MqttResponse) -> Result<bool> {
        Ok(false)
    }
    fn cb_event_sensor_delete(&mut self, response: MqttResponse) -> Result<bool> {
        Ok(false)
    }
    fn cb_event_metric_describe(&mut self, response: MqttResponse) -> Result<bool> {
        // TODO handle error case
        if let [sensor_id, metric_id, ..] = response.get_ids()[..] {
            let message = response.get_message();

            let described_metric = serde_json::from_str::<Metric>(&message)?;
            let sensor = self
                .sensors
                .get_mut(&sensor_id)
                .ok_or_eyre("Sensor not found")?;
            // If metrics was a HashMap, it'd be much simpler :(
            let metric_found = sensor.metrics.iter().find(|m| *m.metric_id() == metric_id);
            if metric_found.is_some() {
                return Ok(false);
            }

            sensor.metrics.push(described_metric.clone());
            self.state_event.emit(SensorStateEvent::NewMetricLoaded {sensor_id, metric: described_metric});

            Ok(true)
        } else {
            Ok(false)
        }
    }
    fn cb_event_metric_create(&mut self, response: MqttResponse) -> Result<bool> {
        // TODO handle error case
        if let [sensor_id, ..] = response.get_ids()[..] {
            let message = response.get_message();
            let metrics_created =
                serde_json::from_str::<Vec<CreateMetricResponsePayload>>(message)
                    .wrap_err_with(|| format!("Failed to deserialize: {}", message))?;
            for payload in &metrics_created {
                self.state_event.emit(SensorStateEvent::NewMetricCreated {
                    sensor_id, metric_id: payload.metric_id.clone(), });
            }
        }
        Ok(false)
    }
    fn cb_event_metric_update(&mut self, response: MqttResponse) -> Result<bool> {
        Ok(false)
    }
    fn cb_event_metric_delete(&mut self, response: MqttResponse) -> Result<bool> {
        Ok(false)
    }
    fn cb_event_push_values(&mut self, response: MqttResponse) -> Result<bool> {
        Ok(false)
    }
}
