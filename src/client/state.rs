use eyre::{OptionExt, Result, WrapErr};

use regex::Regex;

use signals2::{Connect1, Emit1};

use std::collections::{HashMap, HashSet};
use std::ops::Sub;
use std::sync::{Arc, RwLock, Weak};

use uuid::Uuid;

use crate::client::mqtt::MqttClientWrapper;
use crate::model::protocol::{
    CreateMetricResponsePayload, MetricValue, MetricsArrayResponse, PushMetricValueResponse,
};
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
                format!("sensor/{}/delete", sensor_id.to_mqtt()),
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
    pub fn get_id(&self, index: usize) -> Option<Uuid> {
        let topic = self.get_topic();

        let re = Regex::new(r"/([a-f0-9]{32})/").expect("Failed to create regex");

        let result = re
            .captures_iter(&topic)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .map(|m| Uuid::parse_str(m.as_str()).expect("Failed to parse uuid"))
            .nth(index);
        result
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
    ExistingLinkedSensorLoaded(Sensor<LinkedMetric>),

    NewMetricLoaded {
        sensor_id: Uuid,
        metric: Metric,
    },

    NewSensorCreated(Sensor<Metric>),
    NewMetricCreated {
        sensor_id: Uuid,
        metric_id: Uuid,
    },

    SensorUpdated {
        sensor_id: Uuid,
    },
    SensorMetricsUpdated {
        sensor_id: Uuid,
    },

    SensorDeleted {
        sensor_id: Uuid,
    },
    MetricDeleted {
        sensor_id: Uuid,
        metric_id: Uuid,
    },

    SensorNameChanged {
        sensor_id: Uuid,
        name: String,
    },
    MetricNameChanged {
        sensor_id: Uuid,
        metric_id: Uuid,
        name: String,
    },
    MetricValueAnnotationChanged {
        sensor_id: Uuid,
        metric_id: Uuid,
        annotation: String,
    },

    Livedata {
        sensor_id: Uuid,
        metric_id: Uuid,
        value: MetricValue,
    },
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

    pub fn subscribe_to_state_events<Slot>(&mut self, slot: Slot) -> Result<signals2::Connection>
    where
        Slot: Fn(SensorStateEvent) + Send + Sync + 'static,
    {
        Ok(self.state_event.connect(slot))
    }

    pub fn subscribe_to_mqtt_events(&mut self, mqtt_client: &mut MqttClientWrapper) -> Result<()> {
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
        Callback:
            Fn(&mut SensorState, MqttResponse) -> Result<bool> + Clone + Send + Sync + 'static,
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

    fn unassign_callback(&mut self, action: &MqttRequest) {
        let (_, response_topic, error_topic) = action.get_topics();
        self.topic_callbacks
            .remove(&self.full_topic(&response_topic));
        self.topic_callbacks.remove(&self.full_topic(&error_topic));
    }

    fn subscribe_to_sensor_events(&mut self, sensor_id: &Uuid) {
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

        let livedata_topic = format!("sensor/{}/livedata", sensor_id.to_mqtt());
        self.topic_callbacks.insert(
            self.full_topic(&livedata_topic),
            Box::new(Self::cb_event_livedata),
        );
    }

    fn unsubscribe_from_sensor_events(&mut self, sensor_id: &Uuid) {
        self.unassign_callback(&MqttRequest::SensorUpdate(&sensor_id));
        self.unassign_callback(&MqttRequest::SensorDelete(&sensor_id));

        self.unassign_callback(&MqttRequest::MetricCreate(&sensor_id));
        self.unassign_callback(&MqttRequest::MetricUpdate(&sensor_id));
        self.unassign_callback(&MqttRequest::MetricDelete(&sensor_id));

        self.unassign_callback(&MqttRequest::PushValues(&sensor_id));

        let livedata_topic = format!("sensors/{}/livedata", sensor_id.to_mqtt());
        self.topic_callbacks
            .remove(&self.full_topic(&livedata_topic));
    }

    fn cb_event_sensor_list(&mut self, response: MqttResponse) -> Result<bool> {
        // TODO handle error case
        let message = response.get_message();

        let linked_sensors = serde_json::from_str::<Vec<Sensor<LinkedMetric>>>(&message)
            .wrap_err_with(|| format!("Failed to deserialize: {}", &message))?;

        let mut state_changed = false;
        // Merging new sensors
        for linked_sensor in linked_sensors {
            if self.sensors.contains_key(&linked_sensor.sensor_id) {
                // Some metrics might've been deleted
                let deleted_metric_ids = {
                    // Immutable sensor
                    let existing_sensor = self.sensors.get(&linked_sensor.sensor_id).unwrap();
                    // TODO Replace Vec<Metric> with HashMap<Metric> finally!!!
                    let existing_metric_ids = existing_sensor
                        .metrics
                        .iter()
                        .map(|m| m.metric_id().clone())
                        .collect::<HashSet<Uuid>>();
                    let linked_metric_ids = linked_sensor
                        .metrics
                        .iter()
                        .map(|m| m.metric_id.clone())
                        .collect::<HashSet<Uuid>>();
                    existing_metric_ids.sub(&linked_metric_ids)
                };

                for deleted_metric_id in &deleted_metric_ids {
                    self.unassign_callback(&MqttRequest::MetricDescribe {
                        sensor_id: &linked_sensor.sensor_id,
                        metric_id: deleted_metric_id,
                    });
                }
                // Mutable sensor -> no `&mut self` available
                let existing_sensor = self.sensors.get_mut(&linked_sensor.sensor_id).unwrap();
                // The name might have been changed
                if existing_sensor.name != linked_sensor.name {
                    state_changed = true;
                    existing_sensor.name = linked_sensor.name.clone();
                    self.state_event.emit(SensorStateEvent::SensorNameChanged {
                        sensor_id: linked_sensor.sensor_id.clone(),
                        name: existing_sensor.name.clone(),
                    });
                }

                for deleted_metric_id in deleted_metric_ids {
                    existing_sensor.metrics = existing_sensor
                        .metrics
                        .iter()
                        .filter(|m| deleted_metric_id != *m.metric_id())
                        .map(|m| m.clone())
                        .collect();
                    self.state_event.emit(SensorStateEvent::MetricDeleted {
                        sensor_id: linked_sensor.sensor_id.clone(),
                        metric_id: deleted_metric_id,
                    });
                    state_changed = true;
                }

                self.state_event
                    .emit(SensorStateEvent::ExistingLinkedSensorLoaded(linked_sensor));
            } else {
                // Completely new sensor => save it and subscribe to all its events
                let sensor_id = linked_sensor.sensor_id.clone();
                let new_sensor = Sensor {
                    name: linked_sensor.name.clone(),
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

                self.state_event
                    .emit(SensorStateEvent::NewLinkedSensorLoaded(linked_sensor));
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
        // According to https://docs-iot.teamviewer.com/mqtt-api/#533-update
        // there is no info provided with the response, so only the initiator knows
        // what name it was -> name cannot be deduced from the response,
        // given the initiator might be a separate mosquitto_pub process or another client.
        // Thus, re-requesting the entire sensor list as you cannot request concrete sensor
        // details.

        if let Some(sensor_id) = response.get_id(1) {
            if response.get_message() == "Sensor was changed." {
                self.state_event
                    .emit(SensorStateEvent::SensorUpdated { sensor_id });
            }
        }

        Ok(false)
    }
    fn cb_event_sensor_delete(&mut self, response: MqttResponse) -> Result<bool> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#534-delete
        if let Some(sensor_id) = response.get_id(1) {
            if response.get_message() == "Sensor was deleted." {
                self.unsubscribe_from_sensor_events(&sensor_id);
                for metric_id in self.sensors[&sensor_id]
                    .metrics
                    .iter()
                    .map(|m| m.metric_id().clone())
                    .collect::<Vec<Uuid>>()
                {
                    self.unassign_callback(&MqttRequest::MetricDescribe {
                        sensor_id: &sensor_id,
                        metric_id: &metric_id,
                    });
                }
                self.sensors.remove(&sensor_id);
                self.state_event
                    .emit(SensorStateEvent::SensorDeleted { sensor_id });
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn cb_event_metric_describe(&mut self, response: MqttResponse) -> Result<bool> {
        // TODO handle error case
        if let (Some(sensor_id), Some(metric_id)) = (response.get_id(1), response.get_id(2)) {
            let message = response.get_message();

            let described_metric = serde_json::from_str::<Metric>(&message)?;
            let sensor = self
                .sensors
                .get_mut(&sensor_id)
                .ok_or_eyre("Sensor not found")?;

            let mut state_changed = false;

            // If metrics was a HashMap, it'd be much simpler :(
            let metric_found = sensor
                .metrics
                .iter_mut()
                .find(|m| *m.metric_id() == metric_id);
            if let Some(existing_metric) = metric_found {
                if existing_metric.name() != described_metric.name() {
                    existing_metric.rename(described_metric.name().clone());
                    state_changed = true;
                    self.state_event.emit(SensorStateEvent::MetricNameChanged {
                        sensor_id: sensor_id.clone(),
                        metric_id: metric_id.clone(),
                        name: existing_metric.name().clone(),
                    });
                }

                match (existing_metric, described_metric) {
                    (
                        Metric::Custom {
                            value_annotation, ..
                        },
                        Metric::Custom {
                            value_annotation: new_annotation,
                            ..
                        },
                    ) => {
                        if *value_annotation != new_annotation {
                            *value_annotation = new_annotation;
                            state_changed = true;
                            self.state_event
                                .emit(SensorStateEvent::MetricValueAnnotationChanged {
                                    sensor_id: sensor_id.clone(),
                                    metric_id: metric_id.clone(),
                                    annotation: value_annotation.clone(),
                                });
                        }
                    }
                    _ => {}
                }
            } else {
                sensor.metrics.push(described_metric.clone());

                self.state_event.emit(SensorStateEvent::NewMetricLoaded {
                    sensor_id,
                    metric: described_metric,
                });

                state_changed = true;
            }
            Ok(state_changed)
        } else {
            Ok(false)
        }
    }
    fn cb_event_metric_create(&mut self, response: MqttResponse) -> Result<bool> {
        // TODO handle error case
        if let Some(sensor_id) = response.get_id(1) {
            let message = response.get_message();
            let metrics_created = serde_json::from_str::<Vec<CreateMetricResponsePayload>>(message)
                .wrap_err_with(|| format!("Failed to deserialize: {}", message))?;
            for payload in &metrics_created {
                self.assign_callback(
                    &MqttRequest::MetricDescribe {
                        sensor_id: &sensor_id,
                        metric_id: &payload.metric_id,
                    },
                    Self::cb_event_metric_describe,
                );
                self.state_event.emit(SensorStateEvent::NewMetricCreated {
                    sensor_id: sensor_id.clone(),
                    metric_id: payload.metric_id.clone(),
                });
            }
        }
        Ok(false)
    }
    fn cb_event_metric_update(&mut self, response: MqttResponse) -> Result<bool> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#543-update
        if let Some(sensor_id) = response.get_id(1) {
            if response.get_message() == "All metrics were successfully modified." {
                self.state_event
                    .emit(SensorStateEvent::SensorMetricsUpdated { sensor_id });
            }
        }

        Ok(false)
    }
    fn cb_event_metric_delete(&mut self, response: MqttResponse) -> Result<bool> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#544-delete
        // Similar story to `cb_event_sensor_update`
        if let Some(sensor_id) = response.get_id(1) {
            if response.get_message() == "All metrics were successfully deleted." {
                self.state_event
                    .emit(SensorStateEvent::SensorUpdated { sensor_id });
            }
        }
        Ok(false)
    }
    fn cb_event_push_values(&mut self, _: MqttResponse) -> Result<bool> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#51-push-metric-values
        // TODO handle error case, livedata subscription should the rest
        Ok(false)
    }

    fn cb_event_livedata(&mut self, topic: String, message: String) -> Result<bool> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#52-get-metric-values
        let response = MqttResponse::Ok { topic, message };
        if let Some(sensor_id) = response.get_id(1) {
            let message = response.get_message();
            let value_updates =
                serde_json::from_str::<MetricsArrayResponse<PushMetricValueResponse>>(message)
                    .wrap_err_with(|| format!("Failed to deserialize: {}", message))?;
            for value_update in value_updates.metrics {
                self.state_event.emit(SensorStateEvent::Livedata {
                    sensor_id: sensor_id.clone(),
                    metric_id: value_update.metric_id,
                    value: value_update.value,
                });
            }
        }
        Ok(false)
    }
}
