pub mod mqtt;
pub mod state;

use eyre::{eyre, Result, WrapErr};

use serde::{Deserialize, Serialize};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::thread;
use std::thread::JoinHandle;
use uuid::Uuid;

use crate::client::mqtt::MqttClientWrapper;
use crate::client::state::{MqttRequest, SensorState, SensorStateEvent};

use crate::model::protocol::{CreateMetricPayload, CreateSensorRequest, DeleteMetricRequest, MetricValue, MetricsArrayRequest, PingRequest, PingResponse, PushMetricValueRequest, UpdateMetricRequest, UpdateSensorRequest};
use crate::model::sensor::Metric;
use crate::model::ToMqttId;

use std::time::SystemTime;

pub struct SensorVisionClient {
    connector_id: Uuid,
    mqtt_client: Arc<Mutex<MqttClientWrapper>>,

    state: Arc<RwLock<SensorState>>,

    event_stream_thread: Option<JoinHandle<()>>,
    state_event_connection: Option<signals2::Connection>,
    weak_self: Weak<Mutex<SensorVisionClient>>,
}

impl SensorVisionClient {
    pub fn new(
        connector_id: Uuid,
        mqtt_client: Arc<Mutex<MqttClientWrapper>>,
    ) -> Result<Arc<Mutex<Self>>> {
        let state = {
            let mut client = mqtt_client
                .lock()
                .map_err(|_| eyre!("Cannot lock client"))?;
            let sensor_state = SensorState::new(connector_id.clone());
            sensor_state
                .write()
                .unwrap()
                .subscribe_to_mqtt_events(&mut client)?;

            sensor_state
        };

        let result = Arc::new(Mutex::new(Self {
            connector_id,
            mqtt_client,
            state: state.clone(),
            event_stream_thread: None,
            state_event_connection: None,
            weak_self: Weak::new(),
        }));
        let weak_self = Arc::downgrade(&result);
        let weak_self2 = weak_self.clone();

        let (event_sender, event_receiver) = channel::<SensorStateEvent>();

        let event_stream_thread = thread::Builder::new()
            .name("sv-EventHandler".to_owned())
            .spawn(move || loop {
                let event = event_receiver.recv();
                if event.is_err() {
                    break;
                }
                if let Some(this) = weak_self.upgrade() {
                    this.lock().unwrap().state_event_handler(event.unwrap());
                }
            })?;

        let state_event_connection =
            state
                .write()
                .unwrap()
                .subscribe_to_state_events(move |event| {
                    event_sender.send(event).unwrap();
                })?;
        {
            let mut client = result.lock().unwrap();
            client.event_stream_thread = Some(event_stream_thread);
            client.state_event_connection = Some(state_event_connection);
            client.weak_self = weak_self2;
        }

        Ok(result)
    }

    pub fn ping_test(&mut self) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#42-connection-check

        let request = PingRequest {
            request: String::from("Ping!"),
        };
        let pong = self.sync_request_message::<_, PingResponse>(&MqttRequest::Ping, &request)?;

        if pong.answer == "Ping!" {
            Ok(())
        } else {
            Err(eyre!("Unexpected response: {:?}", pong))
        }
    }

    fn state_event_handler(&mut self, event: SensorStateEvent) {
        println!("Sensor state event received: {:?}", event);
        match &event {
            SensorStateEvent::NewLinkedSensorLoaded(linked_sensor) => {
                // TODO Fire UI Event
                for linked_metric in &linked_sensor.metrics {
                    self.async_raw_message(
                        &MqttRequest::MetricDescribe {
                            sensor_id: &linked_sensor.sensor_id,
                            metric_id: &linked_metric.metric_id,
                        },
                        None,
                    )
                    .expect("Failed to send message");
                }
            }
            SensorStateEvent::ExistingLinkedSensorLoaded(linked_sensor) => {
                for linked_metric in &linked_sensor.metrics {
                    self.async_raw_message(
                        &MqttRequest::MetricDescribe {
                            sensor_id: &linked_sensor.sensor_id,
                            metric_id: &linked_metric.metric_id,
                        },
                        None,
                    )
                    .expect("Failed to send message");
                }
            }
            SensorStateEvent::NewSensorCreated(..) => {
                // TODO Fire UI Event
            }
            SensorStateEvent::NewMetricLoaded { .. } => {
                // TODO Fire UI Event
            }
            SensorStateEvent::NewMetricCreated {
                sensor_id,
                metric_id,
            } => {
                // TODO Fire UI Event
                self.async_raw_message(
                    &MqttRequest::MetricDescribe {
                        sensor_id,
                        metric_id,
                    },
                    None,
                )
                .expect("Failed to send message");
            }
            // There is no other way to get sensor/metric update details
            // rather than loading all the sensor again :(
            SensorStateEvent::SensorUpdated { .. } => {
                // TODO Fire UI Event
                self.load_sensors().expect("Failed to load sensors");
            }
            SensorStateEvent::SensorDeleted { .. } => {
                // TODO Fire UI Event
            }
            SensorStateEvent::MetricDeleted { .. } => {
                // TODO Fire UI Event
            }
            SensorStateEvent::SensorNameChanged { .. } => {
                // TODO Fire UI Event
            }
            SensorStateEvent::MetricNameChanged { .. } => {
                // TODO Fire UI Event
            }
            SensorStateEvent::MetricValueAnnotationChanged { .. } => {
                // TODO Fire UI Event
            }
            SensorStateEvent::Livedata {.. } => {
                // TODO Fire UI Event
            }
        }
    }

    pub fn sensor_id_by_name(&self, sensor_name: &str) -> Option<Uuid> {
        let state = self.state.read().unwrap();
        let found = state
            .sensors
            .iter()
            .find(|(_, sensor)| sensor.name == sensor_name);
        if let Some((sensor_id, _)) = found {
            Some(sensor_id.clone())
        } else {
            None
        }
    }

    pub fn metric_id_by_name(&self, sensor_id: &Uuid, name: &str) -> Option<Uuid> {
        let state = self.state.read().unwrap();
        if let Some(sensor) = state.sensors.get(sensor_id) {
            let found = sensor.metrics.iter().find(|metric| metric.name() == name);
            if let Some(metric) = found {
                return Some(metric.metric_id().clone());
            }
        }
        None
    }

    pub fn load_sensors(&mut self) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#532-list
        self.async_raw_message(&MqttRequest::SensorList, None)
    }

    pub fn create_sensor(&mut self, name: &str) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#531-create
        let request = CreateSensorRequest {
            name: String::from(name),
        };
        self.async_request_message(&MqttRequest::SensorCreate, &request)
    }

    pub fn update_sensor(
        &mut self,
        sensor_id: &Uuid,
        name: &str,
        state: Option<bool>,
    ) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#533-update
        let request = UpdateSensorRequest {
            name: String::from(name),
            state: state.map(|x| x as u8),
        };

        self.async_request_message(&MqttRequest::SensorUpdate(sensor_id), &request)
    }

    pub fn delete_sensor(&mut self, sensor_id: &Uuid) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#534-delete
        self.async_raw_message(&MqttRequest::SensorDelete(sensor_id), None)
    }

    pub fn dump_sensors(&self) -> Result<String> {
        serde_json::to_string_pretty(
            &self
                .state
                .read()
                .unwrap()
                .sensors
                .iter()
                .map(|(_, s)| s.clone())
                .collect::<Vec<_>>(),
        )
        .wrap_err("Failed to dump sensors")
    }

    pub fn create_metrics(&mut self, sensor_id: &Uuid, metrics: &Vec<Metric>) -> Result<()> {
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

        self.async_request_message(&MqttRequest::MetricCreate(sensor_id), &request)
    }

    pub fn update_metric(
        &mut self,
        sensor_id: &Uuid,
        metric_id: &Uuid,
        name: Option<&str>,
        value_annotation: Option<&str>,
    ) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#533-update
        let request = MetricsArrayRequest::one(UpdateMetricRequest {
            metric_id: metric_id.clone(),
            name,
            value_annotation,
        });

        self.async_request_message(&MqttRequest::MetricUpdate(sensor_id), &request)
    }

    pub fn delete_metric(&mut self, sensor_id: &Uuid, metric_id: &Uuid) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#543-update
        let request = MetricsArrayRequest::one(DeleteMetricRequest {
            metric_id: metric_id.clone(),
        });

        self.async_request_message(&MqttRequest::MetricDelete(sensor_id), &request)
    }

    pub fn push_value(
        &mut self,
        sensor_id: &Uuid,
        metric_id: &Uuid,
        value: &MetricValue,
        timestamp: Option<&SystemTime>,
    ) -> Result<()> {
        // According to https://docs-iot.teamviewer.com/mqtt-api/#51-push-metric-values
        let timestamp = timestamp.map(|ts| {
            ts.duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        });

        let request = MetricsArrayRequest::one(PushMetricValueRequest {
            metric_id : metric_id.clone(),
            value: value.clone(),
            timestamp,
        });
        self.async_request_message(&MqttRequest::PushValues(&sensor_id), &request)
    }

    fn async_raw_message(&mut self, action: &MqttRequest, payload: Option<String>) -> Result<()> {
        let client = &mut self
            .mqtt_client
            .lock()
            .map_err(|_| eyre!("Cannot lock client"))?;

        let (topic, _, _) = action.get_topics();
        let connector_id = self.connector_id.to_mqtt();
        let full_topic = format!("/v1.0/{}/{}", connector_id, topic);

        let message = payload.unwrap_or(String::from("{}"));

        client.async_message(&full_topic, &message)
    }

    fn async_request_message<Request>(&mut self, action: &MqttRequest, body: &Request) -> Result<()>
    where
        Request: Serialize,
    {
        let request_serialized = serde_json::to_string(body)?;
        self.async_raw_message(action, Some(request_serialized))
    }

    fn sync_raw_message(
        &mut self,
        action: &MqttRequest,
        message: Option<String>,
    ) -> Result<String> {
        let client = &mut self
            .mqtt_client
            .lock()
            .map_err(|_| eyre!("Cannot lock client"))?;

        let message = message.unwrap_or(String::from("{}"));

        let (topic, response_topic, error_topic) = action.get_topics();

        let connector_id = self.connector_id.to_mqtt();
        let full_topic = format!("/v1.0/{}/{}", connector_id, topic);
        let full_response_topic = format!("/v1.0/{}/{}", connector_id, response_topic);
        let full_error_topic = format!("/v1.0/{}/{}", connector_id, error_topic);
        client.sync_request(
            &full_topic,
            &full_response_topic,
            &full_error_topic,
            &message,
        )
    }

    fn sync_request_message<Request, Response>(
        &mut self,
        action: &MqttRequest,
        body: &Request,
    ) -> Result<Response>
    where
        Request: Serialize,
        Response: for<'a> Deserialize<'a>,
    {
        let request_serialized = serde_json::to_string(body)?;
        let response_serialized = self.sync_raw_message(action, Some(request_serialized))?;
        Ok(serde_json::from_str(&response_serialized)?)
    }
}
