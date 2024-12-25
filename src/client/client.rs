use std::fmt::Debug;
use std::time::SystemTime;
use eyre::{eyre, Result, WrapErr};

use serde::de::Deserialize;
use serde::ser::Serialize;

use tokio::sync::broadcast;
use tokio::sync::oneshot;

use crate::client::mqtt::{MqttActorHandle, MqttMessage};
use crate::client::state::{MqttScheme, SensorStateEvent, SensorStateQuery, SensorStateQueryProto, SensorStateActorHandle, Sensors};
use crate::model::{ConnectorId, MetricId, SensorId};
use crate::model::protocol::{CreateMetricPayload, CreateSensorRequest, DeleteMetricRequest, MetricValue, MetricsArrayRequest, PingRequest, PingResponse, PushMetricValueRequest, UpdateMetricRequest, UpdateSensorRequest};
use crate::model::sensor::Metric;

#[derive(Debug, Clone)]
pub struct SensorVisionClient {
    connector_id: ConnectorId,

    mqtt_actor: MqttActorHandle,
    state_actor: SensorStateActorHandle,

    event_repeater: broadcast::Sender<SensorStateEvent>,
}

impl SensorVisionClient {
    pub fn new(connector_id: ConnectorId) -> Result<Self> {
        let mqtt_actor = MqttActorHandle::new()?;

        let (event_sender, event_receiver) = broadcast::channel::<SensorStateEvent>(100);
        let event_repeater = event_sender.clone();
        let state_actor = SensorStateActorHandle::new(connector_id, event_sender)?;
        let instance = Self {
            connector_id,
            mqtt_actor,
            state_actor,
            event_repeater,
        };

        tokio::task::Builder::new()
            .name("sensors state event loop")
            .spawn(state_event_loop(instance.clone(), event_receiver))?;

        Ok(instance)
    }

    pub fn state_event_receiver(&self) -> broadcast::Receiver<SensorStateEvent> {
        self.event_repeater.subscribe()
    }

    pub async fn state_query<T: Debug>(
        &self,
        query: SensorStateQuery,
        receiver: oneshot::Receiver<T>
    ) -> T {
        self.state_actor.state_query(query);
        receiver.await.expect("Receiving failed")
    }

    pub async fn sensor_id_by_name(&self, sensor_name: &str) -> Option<SensorId> {
        let (tx, rx) = oneshot::channel();
        let query = SensorStateQuery::GetSensorIdByName(SensorStateQueryProto{
            request: sensor_name.to_owned(),
            respond_to: tx,
        });
        self.state_query(query, rx).await
    }

    pub async fn metric_id_by_name(&self, sensor_id: SensorId, metric_name: &str) -> Option<MetricId> {
        let (tx, rx) = oneshot::channel();
        let query = SensorStateQuery::GetMetricIdByName(SensorStateQueryProto{
            request: (sensor_id, metric_name.to_owned()),
            respond_to: tx,
        });
        self.state_query(query, rx).await
    }

    pub async fn ping_test(&self) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#42-connection-check
        let request = PingRequest {
            request: String::from("Ping!"),
        };

        let pong: PingResponse = self.request(MqttScheme::Ping, &request).await?;

        if pong.answer == "Ping!" {
            Ok(())
        } else {
            Err(eyre!("Unexpected response: {:?}", pong))
        }
    }

    pub fn create_sensor(&self, name: &str) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#531-create
        let request = CreateSensorRequest {
            name: String::from(name),
        };
        self.message(MqttScheme::SensorCreate, &request)
    }

    pub fn update_sensor(
        &self,
        sensor_id: SensorId,
        name: &str,
        state: Option<bool>
    ) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#533-update
        let request = UpdateSensorRequest {
            name: String::from(name),
            state: state.map(|x| x as u8),
        };
        self.message(MqttScheme::SensorUpdate(sensor_id), &request)
    }

    pub fn delete_sensor(&self, sensor_id: SensorId) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#534-delete
        self.raw_message(MqttScheme::SensorDelete(sensor_id), None)
    }

    pub async fn get_sensors(&self) -> Sensors {
        let (tx, rx) = oneshot::channel();
        let query = SensorStateQuery::GetStateSnapshot(SensorStateQueryProto{
            request: (),
            respond_to: tx,
        });
        self.state_query(query, rx).await
    }

    pub async fn dump_sensors(&self) -> Result<String> {
        let sensors = self.get_sensors().await;
        serde_json::to_string_pretty(&sensors).wrap_err("Failed to dump sensors")
    }

    pub fn load_sensors(&self) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#532-list
        self.raw_message(MqttScheme::SensorList, None)
    }

    pub fn create_metrics(&self, sensor_id: SensorId, metrics: &Vec<Metric>) -> Result<()> {
        let request = MetricsArrayRequest::many(
            metrics
                .iter()
                .enumerate()
                .map(|(i, metric)| CreateMetricPayload {
                    metric: metric.clone(),
                    matching_id: i + 1,
                })
                .collect(),
        );

        self.message(MqttScheme::MetricCreate(sensor_id), &request)
    }

    pub fn update_metric(
        &self,
        sensor_id: SensorId,
        metric_id: MetricId,
        name: Option<String>,
        value_annotation: Option<String>,
    ) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#533-update
        let request = MetricsArrayRequest::one(UpdateMetricRequest {
            metric_id: metric_id.clone(),
            name,
            value_annotation,
        });
        self.message(MqttScheme::MetricUpdate(sensor_id), &request)
    }

    pub fn delete_metric(
        &self,
        sensor_id: SensorId,
        metric_id: MetricId,
    ) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#543-update
        let request = MetricsArrayRequest::one(DeleteMetricRequest {
            metric_id: metric_id.clone(),
        });
        self.message(MqttScheme::SensorDelete(sensor_id), &request)
    }

    pub fn push_value(
        &self,
        sensor_id: SensorId,
        metric_id: MetricId,
        value: MetricValue,
        timestamp: Option<SystemTime>,
    ) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#51-push-metric-values
        let timestamp = timestamp.map(|ts| {
            ts.duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        });

        let request = MetricsArrayRequest::one(PushMetricValueRequest {
            metric_id,
            value,
            timestamp,
        });

        self.message(MqttScheme::PushValues(sensor_id), &request)
    }

    fn raw_message(&self, scheme: MqttScheme, payload: Option<String>) -> Result<()> {
        let (topic, _, _) = scheme.get_topics();
        let full_topic = format!("/v1.0/{}/{}", self.connector_id, topic);

        let message = payload.unwrap_or(String::from("{}"));

        self.mqtt_actor
            .one_way_message(MqttMessage {
                topic: full_topic,
                message,
            })
    }

    fn message<Blueprint: Serialize>(
        &self,
        scheme: MqttScheme,
        body: &Blueprint,
    ) -> Result<()> {
        let body_serialized = serde_json::to_string(body)?;
        self.raw_message(scheme, Some(body_serialized))
    }

    async fn raw_request(&self, scheme: MqttScheme, message: Option<String>) -> Result<String> {
        let (topic, response_topic, error_topic) = scheme.get_topics();

        let full_topic = format!("/v1.0/{}/{}", self.connector_id, topic);
        let full_response_topic = format!("/v1.0/{}/{}", self.connector_id, response_topic);
        let full_error_topic = format!("/v1.0/{}/{}", self.connector_id, error_topic);

        let message = message.unwrap_or(String::from("{}"));

        self.mqtt_actor
            .request(
                MqttMessage {
                    topic: full_topic,
                    message,
                },
                full_response_topic,
                full_error_topic,
            )
            .await
    }

    async fn request<Request: Serialize, Response: for<'a> Deserialize<'a>>(
        &self,
        scheme: MqttScheme,
        request: &Request,
    ) -> Result<Response> {
        let request_serialized = serde_json::to_string(request)?;
        let response_serialized = self.raw_request(scheme, Some(request_serialized)).await?;
        Ok(serde_json::from_str(&response_serialized)?)
    }

    async fn state_event_handler(&mut self, event: SensorStateEvent) {
        use crate::client::state::SensorStateEvent::*;

        match &event {
            NewLinkedSensorLoaded(linked_sensor) | ExistingLinkedSensorLoaded(linked_sensor) => {
                // TODO Fire UI Event
                for linked_metric in &linked_sensor.metrics {
                    self.raw_message(
                        MqttScheme::MetricDescribe(
                            linked_sensor.sensor_id,
                            linked_metric.metric_id,
                        ),
                        None,
                    )
                    .expect("Failed to send message");
                }
            }

            NewMetricCreated {
                sensor_id,
                metric_id,
            } => {
                self.raw_message(MqttScheme::MetricDescribe(*sensor_id, *metric_id), None)
                    .expect("Failed to send message");
            }

            SensorUpdated { .. } => {
                // There is no other way to get sensor/metric update details
                // rather than reloading all the sensors again :(
                self.load_sensors().expect("Failed to reload sensors");
            }

            SensorMetricsUpdated { sensor_id } => {
                let (tx, rx) = oneshot::channel();
                let query = SensorStateQuery::GetMetricIds(SensorStateQueryProto{
                    request: *sensor_id,
                    respond_to: tx,
                });
                let Some(metric_ids) = self.state_query(query, rx).await else {
                    return;
                };
                for metric_id in metric_ids {
                    self.raw_message(MqttScheme::MetricDescribe(*sensor_id, metric_id), None)
                        .expect("Failed to send message");
                }
            }
            _ => {}
        };
    }
}

async fn state_event_loop(
    mut client: SensorVisionClient,
    mut event_receiver: broadcast::Receiver<SensorStateEvent>,
) {
    while let Ok(event) = event_receiver.recv().await {
        client.state_event_handler(event).await;
    }
}
