use actix::{Actor, Context, Handler, Message, WeakRecipient};

use eyre::{OptionExt, Result, WrapErr};

use strum::IntoEnumIterator;

use std::ops::Sub;
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::client::state::MqttScheme;
use crate::client::mqtt::MqttEvent;
use crate::model::sensor::{LinkedMetric, Metric, Sensor};
use crate::model::{MetricId, MqttId, SensorId};
use crate::model::protocol::{CreateMetricResponsePayload, ErrorResponse, MetricValue, MetricsArrayResponse, PushMetricValueResponse};

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub enum SensorStateEvent {
    NewLinkedSensorLoaded(Sensor<LinkedMetric>),
    ExistingLinkedSensorLoaded(Sensor<LinkedMetric>),

    NewMetricLoaded {
        sensor_id: SensorId,
        metric: Metric,
    },

    NewSensorCreated(Sensor<Metric>),
    NewMetricCreated {
        sensor_id: SensorId,
        metric_id: MetricId,
    },

    SensorUpdated {
        sensor_id: SensorId,
    },
    SensorMetricsUpdated {
        sensor_id: SensorId,
    },

    SensorDeleted {
        sensor_id: SensorId,
    },
    MetricDeleted {
        sensor_id: SensorId,
        metric_id: MetricId,
    },

    SensorNameChanged {
        sensor_id: SensorId,
        name: String,
    },
    MetricNameChanged {
        sensor_id: SensorId,
        metric_id: MetricId,
        name: String,
    },
    MetricValueAnnotationChanged {
        sensor_id: SensorId,
        metric_id: MetricId,
        annotation: String,
    },

    Livedata {
        sensor_id: SensorId,
        metric_id: MetricId,
        value: MetricValue,
        timestamp: u64,
    },

    Error {
        message: String,
        code: i32,
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SubscribeToStateEvents(pub WeakRecipient<SensorStateEvent>);

// TODO Replace with in-memory SQLite
pub type Sensors = BTreeMap<SensorId, Sensor<Metric>>;

#[derive(Default)]
pub struct SensorsStateActor {
    pub(super) sensors: Sensors,

    // For speeding up
    topic_schemes: HashMap<String, MqttScheme>,

    event_subscribers: Vec<WeakRecipient<SensorStateEvent>>,
}

impl SensorsStateActor {
    pub fn new() -> Self {
        let mut result = Self::default();

        for scheme in MqttScheme::iter() {
            result.init_scheme(scheme);
        }

        result
    }

    fn init_scheme(&mut self, scheme: MqttScheme) {
        let (_, response, error) = scheme.get_templates();
        self.topic_schemes.insert(response.to_owned(), scheme);
        self.topic_schemes.insert(error.to_owned(), scheme);
    }

    fn emit_event(&self, event: SensorStateEvent) {
        for subscriber in &self.event_subscribers {
            if let Some(subscriber) = subscriber.upgrade() {
                subscriber.do_send(event.clone());
            }
        }
    }

    fn emit_events(&self, events: Vec<SensorStateEvent>) {
        for event in events {
            self.emit_event(event);
        }
    }

    fn event_sensor_list(&mut self, _: Vec<MqttId>, message: String) -> Result<()> {
        let linked_sensors = serde_json::from_str::<Vec<Sensor<LinkedMetric>>>(&message)
            .wrap_err_with(|| format!("Failed to deserialize: {}", &message))?;

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
                        .collect::<HashSet<MetricId>>();
                    let linked_metric_ids = linked_sensor
                        .metrics
                        .iter()
                        .map(|m| m.metric_id.clone())
                        .collect::<HashSet<MetricId>>();
                    existing_metric_ids.sub(&linked_metric_ids)
                };

                let mut events = Vec::new();

                // Mutable sensor -> no `&mut self` available
                let existing_sensor = self.sensors.get_mut(&linked_sensor.sensor_id).unwrap();
                // The name might have been changed
                if existing_sensor.name != linked_sensor.name {
                    existing_sensor.name = linked_sensor.name.clone();
                    events.push(SensorStateEvent::SensorNameChanged {
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
                    events.push(SensorStateEvent::MetricDeleted {
                        sensor_id: linked_sensor.sensor_id.clone(),
                        metric_id: deleted_metric_id,
                    });
                }

                self.emit_events(events);
                self.emit_event(SensorStateEvent::ExistingLinkedSensorLoaded(linked_sensor));
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

                self.emit_event(SensorStateEvent::NewLinkedSensorLoaded(linked_sensor));
            }
        }

        Ok(())
    }
    fn event_sensor_create(&mut self, _: Vec<MqttId>, message: String) -> Result<()> {
        let new_sensor = serde_json::from_str::<Sensor<Metric>>(&message)
            .wrap_err_with(|| format!("Failed to deserialize: {}", &message))?;

        let sensor_id = new_sensor.sensor_id.clone();

        self.sensors.insert(sensor_id, new_sensor.clone());

        self.emit_event(SensorStateEvent::NewSensorCreated(new_sensor));
        Ok(())
    }
    fn event_sensor_update(&mut self, mut ids: Vec<MqttId>, message: String) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#533-update
        // there is no info provided with the response, so only the initiator knows
        // what name it was -> name cannot be deduced from the response,
        // given the initiator might be a separate mosquitto_pub process or another client.
        // Thus, re-requesting the entire sensor list as you cannot request concrete sensor
        // details.

        if let Some(sensor_id) = ids.pop() {
            if message == "Sensor was changed." {
                self.emit_event(SensorStateEvent::SensorUpdated { sensor_id });
            }
        }
        Ok(())
    }
    fn event_sensor_delete(&mut self, mut ids: Vec<MqttId>, message: String) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#534-delete
        if let Some(sensor_id) = ids.pop() {
            if message == "Sensor was deleted." {
                self.sensors.remove(&sensor_id);
                self.emit_event(SensorStateEvent::SensorDeleted { sensor_id });
            }
        }
        Ok(())
    }
    fn event_metric_describe(&mut self, ids: Vec<MqttId>, message: String) -> Result<()> {
        let (Some(sensor_id), Some(metric_id)) = (ids.get(0), ids.get(1)) else {
            return Ok(());
        };

        let described_metric = serde_json::from_str::<Metric>(&message)?;
        let sensor = self
            .sensors
            .get_mut(&sensor_id)
            .ok_or_eyre("Sensor not found")?;

        let mut events = Vec::new();
        // If metrics was a HashMap, it'd be much simpler :(
        let metric_found = sensor
            .metrics
            .iter_mut()
            .find(|m| *m.metric_id() == *metric_id);
        if let Some(existing_metric) = metric_found {
            if existing_metric.name() != described_metric.name() {
                existing_metric.rename(described_metric.name().clone());
                events.push(SensorStateEvent::MetricNameChanged {
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
                        log::debug!(
                            "Metric annotation changed: {} -> {}",
                            value_annotation,
                            new_annotation
                        );
                        *value_annotation = new_annotation;
                        events.push(SensorStateEvent::MetricValueAnnotationChanged {
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

            events.push(SensorStateEvent::NewMetricLoaded {
                sensor_id: *sensor_id,
                metric: described_metric,
            });
        }

        self.emit_events(events);
        Ok(())
    }
    fn event_metric_create(&mut self, mut ids: Vec<MqttId>, message: String) -> Result<()> {
        if let Some(sensor_id) = ids.pop() {
            let metrics_created =
                serde_json::from_str::<Vec<CreateMetricResponsePayload>>(&message)
                    .wrap_err_with(|| format!("Failed to deserialize: {}", message))?;
            for payload in &metrics_created {
                self.emit_event(SensorStateEvent::NewMetricCreated {
                    sensor_id: sensor_id.clone(),
                    metric_id: payload.metric_id.clone(),
                });
            }
        }
        Ok(())
    }
    fn event_metric_update(&mut self, mut ids: Vec<MqttId>, message: String) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#543-update
        if let Some(sensor_id) = ids.pop() {
            if message == "All metrics were successfully modified." {
                self.emit_event(SensorStateEvent::SensorMetricsUpdated { sensor_id });
            }
        }
        Ok(())
    }
    fn event_metric_delete(&mut self, mut ids: Vec<MqttId>, message: String) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#544-delete
        if let Some(sensor_id) = ids.pop() {
            if message == "All metrics were successfully deleted." {
                self.emit_event(SensorStateEvent::SensorUpdated { sensor_id });
            }
        }
        Ok(())
    }
    fn event_push_values(&mut self, _: Vec<MqttId>, _: String) -> Result<()> {
        Ok(())
    }
    fn event_ping(&mut self, _: Vec<MqttId>, _: String) -> Result<()> {
        Ok(())
    }

    fn event_livedata(&mut self, mut ids: Vec<MqttId>, message: String) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#52-get-metric-values
        if let Some(sensor_id) = ids.pop() {
            let value_updates =
                serde_json::from_str::<MetricsArrayResponse<PushMetricValueResponse>>(&message)
                    .wrap_err_with(|| format!("Failed to deserialize: {}", message))?;
            for value_update in value_updates.metrics {
                self.emit_event(SensorStateEvent::Livedata {
                    sensor_id: sensor_id.clone(),
                    metric_id: value_update.metric_id,
                    value: value_update.value,
                    timestamp: value_updates.timestamp.unwrap(),
                });
            }
        }
        Ok(())
    }
}

impl Handler<MqttEvent> for SensorsStateActor {
    type Result = ();

    fn handle(&mut self, MqttEvent(msg): MqttEvent, _: &mut Self::Context) -> Self::Result {
        let short_topic = msg.topic[39..].to_owned(); // cut /v1.0/6d69c58223fb44a7b76ae61a18faf37c/ off
        let (mqtt_ids, pattern) = MqttScheme::extract_ids_and_pattern(&short_topic);
        // There is no such MqttScheme cause it's an "event"
        if pattern == "sensor/:mqttid:/livedata" {
            let _ = self.event_livedata(mqtt_ids, msg.message);
            return;
        }

        if let Some(scheme) = self.topic_schemes.get(&pattern) {
            use MqttScheme::*;
            let (_, response_pattern, _) = scheme.get_templates();
            if response_pattern != pattern {
                if let Ok(error_response) = serde_json::from_str::<ErrorResponse>(&msg.message) {
                    self.emit_event(SensorStateEvent::Error {
                        message: error_response.message,
                        code: error_response.code,
                    });
                } else {
                    log::error!(
                        "Error in topic '{}', message '{}'",
                        msg.topic,
                        msg.message,
                    );
                }
                return;
            };

            let result = match scheme {
                PushValues(..) => self.event_push_values(mqtt_ids, msg.message),
                SensorList => self.event_sensor_list(mqtt_ids, msg.message),
                SensorCreate => self.event_sensor_create(mqtt_ids, msg.message),
                SensorUpdate(..) => self.event_sensor_update(mqtt_ids, msg.message),
                SensorDelete(..) => self.event_sensor_delete(mqtt_ids, msg.message),
                MetricDescribe(..) => self.event_metric_describe(mqtt_ids, msg.message),
                MetricCreate(..) => self.event_metric_create(mqtt_ids, msg.message),
                MetricUpdate(..) => self.event_metric_update(mqtt_ids, msg.message),
                MetricDelete(..) => self.event_metric_delete(mqtt_ids, msg.message),
                Ping => self.event_ping(mqtt_ids, msg.message),
            };
            if let Err(err) = result {
                log::error!("Error while processing mqtt event {}", err)
            }
        }
    }
}

impl Handler<SubscribeToStateEvents> for SensorsStateActor {
    type Result = ();

    fn handle(&mut self, msg: SubscribeToStateEvents, _: &mut Self::Context) -> Self::Result {
        self.event_subscribers.push(msg.0);
    }
}

impl Actor for SensorsStateActor {
    type Context = Context<Self>;
}
