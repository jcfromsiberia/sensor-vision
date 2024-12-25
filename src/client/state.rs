use eyre::{eyre, OptionExt, Result, WrapErr};

use regex::Regex;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Debug;
use std::ops::Sub;

use strum::{EnumIter, IntoEnumIterator};

use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::client::mqtt::make_mqtt_listener;
use crate::model::protocol::{
    CreateMetricResponsePayload, MetricValue, MetricsArrayResponse, PushMetricValueResponse,
};
use crate::model::sensor::{LinkedMetric, Metric, Sensor};
use crate::model::{ConnectorId, MetricId, MqttId, SensorId};

use crate::client::mqtt::MqttMessage;

#[derive(Debug, Clone)]
pub struct SensorStateActorHandle {
    sender: mpsc::UnboundedSender<StateActorCommand>,
}

impl SensorStateActorHandle {
    pub fn new(
        connector_id: ConnectorId,
        state_event_sender: broadcast::Sender<SensorStateEvent>,
    ) -> Result<Self> {
        let events_topic = format!("/v1.0/{}/#", connector_id);
        let mqtt_event_receiver = make_mqtt_listener(events_topic, "sv_events".to_owned())?;
        let (actor_command_sender, actor_command_receiver) = mpsc::unbounded_channel::<StateActorCommand>();

        let actor = SensorStateActor::new(actor_command_receiver, state_event_sender);

        tokio::task::Builder::new()
            .name("sensors state actor loop")
            .spawn(state_actor_loop(actor, mqtt_event_receiver))?;

        Ok(Self {
            sender: actor_command_sender,
        })
    }

    pub fn state_query(&self, query: SensorStateQuery) {
        let command = StateActorCommand::StateQuery(query);
        self.sender.send(command).expect("Sending failed");
    }
}

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

    fn render_topic(template: &str, args: &[String]) -> String {
        let mut result = template.to_string();
        for arg in args {
            result = result.replacen(":mqttid:", arg, 1);
        }
        result
    }

    fn extract_ids_and_pattern(topic: &str) -> (Vec<MqttId>, String) {
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

#[derive(Debug, Clone)]
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
}

pub type Sensors = BTreeMap<SensorId, Sensor<Metric>>;

#[derive(Debug)]
pub struct SensorStateQueryProto<Request, Response> {
    pub request: Request,
    pub respond_to: oneshot::Sender<Response>,
}

type Q<T, U> = SensorStateQueryProto<T, U>;

#[derive(Debug)]
pub enum SensorStateQuery {
    GetStateSnapshot(Q<(), Sensors>),
    GetMetricIds(Q<SensorId, Option<HashSet<MetricId>>>),
    GetSensorIdByName(Q<String, Option<SensorId>>),
    GetMetricIdByName(Q<(SensorId, String), Option<MetricId>>),
}

#[derive(Debug)]
enum StateActorCommand {
    ProcessMqttEvent { message: MqttMessage },
    StateQuery(SensorStateQuery),
}

#[derive(Debug)]
struct SensorStateActor {
    receiver: mpsc::UnboundedReceiver<StateActorCommand>,
    event_sender: broadcast::Sender<SensorStateEvent>,

    // For speeding up
    topic_schemes: HashMap<String, MqttScheme>,

    sensors: Sensors,
}

impl SensorStateActor {
    fn new(
        receiver: mpsc::UnboundedReceiver<StateActorCommand>,
        event_sender: broadcast::Sender<SensorStateEvent>,
    ) -> Self {
        let mut result = Self {
            receiver,
            event_sender,
            topic_schemes: HashMap::new(),
            sensors: Sensors::new(),
        };

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

    fn handle_command(&mut self, command: StateActorCommand) {
        use StateActorCommand::*;

        match command {
            ProcessMqttEvent { message } => {
                if let Err(_) = self.handle_mqtt_event(message) {
                    // Log error
                }
            }

            StateQuery(query) => self.handle_state_query(query),
        }
    }

    fn respond<T: Debug>(value: T, sender: oneshot::Sender<T>) {
        sender.send(value).expect("Responding failed");
    }

    fn handle_state_query(&self, query: SensorStateQuery) {
        use SensorStateQuery::*;

        match query {
            GetStateSnapshot(Q { respond_to, .. }) => {
                Self::respond(self.sensors.clone(), respond_to);
            }

            GetMetricIds(Q {
                request: sensor_id,
                respond_to,
            }) => {
                let Some(sensor) = self.sensors.get(&sensor_id) else {
                    Self::respond(None, respond_to);
                    return;
                };
                let metric_ids = sensor
                    .metrics
                    .iter()
                    .map(|m| m.metric_id().clone())
                    .collect();
                Self::respond(Some(metric_ids), respond_to);
            }

            GetSensorIdByName(Q {
                request: sensor_name,
                respond_to,
            }) => {
                let Some((sensor_id, _)) = self
                    .sensors
                    .iter()
                    .find(|(_, sensor)| sensor.name == sensor_name)
                else {
                    Self::respond(None, respond_to);
                    return;
                };
                Self::respond(Some(*sensor_id), respond_to);
            }

            GetMetricIdByName(Q {
                request: (sensor_id, metric_name),
                respond_to,
            }) => {
                if let Some(sensor) = self.sensors.get(&sensor_id) {
                    if let Some(metric) = sensor
                        .metrics
                        .iter()
                        .find(|metric| metric.name() == &metric_name)
                    {
                        Self::respond(Some(metric.metric_id().clone()), respond_to);
                        return;
                    }
                }
                Self::respond(None, respond_to);
            }
        }
    }

    fn emit_event(&self, event: SensorStateEvent) {
        let _ = self.event_sender.send(event);
    }

    fn emit_events(&self, events: Vec<SensorStateEvent>) {
        for event in events {
            self.emit_event(event);
        }
    }

    fn handle_mqtt_event(&mut self, message: MqttMessage) -> Result<()> {
        let short_topic = message.topic[39..].to_owned(); // cut /v1.0/6d69c58223fb44a7b76ae61a18faf37c/ off
        let (mqtt_ids, pattern) = MqttScheme::extract_ids_and_pattern(&short_topic);
        // There is no such MqttScheme cause it's an "event"
        if pattern == "sensor/:mqttid:/livedata" {
            return Ok(self.event_livedata(mqtt_ids, message.message)?);
        }

        if let Some(scheme) = self.topic_schemes.get(&pattern) {
            use MqttScheme::*;
            let (_, response_pattern, _) = scheme.get_templates();
            if response_pattern != pattern {
                return Err(eyre!(
                    "Error in topic '{}', message '{}'",
                    message.topic,
                    message.message
                ));
            };
            Ok(match scheme {
                PushValues(..) => self.event_push_values(mqtt_ids, message.message)?,
                SensorList => self.event_sensor_list(mqtt_ids, message.message)?,
                SensorCreate => self.event_sensor_create(mqtt_ids, message.message)?,
                SensorUpdate(..) => self.event_sensor_update(mqtt_ids, message.message)?,
                SensorDelete(..) => self.event_sensor_delete(mqtt_ids, message.message)?,
                MetricDescribe(..) => self.event_metric_describe(mqtt_ids, message.message)?,
                MetricCreate(..) => self.event_metric_create(mqtt_ids, message.message)?,
                MetricUpdate(..) => self.event_metric_update(mqtt_ids, message.message)?,
                MetricDelete(..) => self.event_metric_delete(mqtt_ids, message.message)?,
                Ping => self.event_ping(mqtt_ids, message.message)?,
            })
        } else {
            // TODO Log unhandled
            Err(eyre!("{}", message.message))
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

async fn state_actor_loop(
    mut actor: SensorStateActor,
    mut event_listener: mpsc::UnboundedReceiver<MqttMessage>,
) {
    loop {
        tokio::select! {
            Some(message) = event_listener.recv() => {
                actor.handle_command(StateActorCommand::ProcessMqttEvent{
                    message
                });
            },
            Some(command) = actor.receiver.recv() => {
                actor.handle_command(command);
            },
            else => {
                break;
            }
        }
    }
}
