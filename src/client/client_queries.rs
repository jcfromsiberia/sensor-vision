use actix::{Handler, Message, ResponseFuture};

use eyre::{eyre, Context, Result};

use futures::FutureExt;

use std::time::SystemTime;

use crate::client::client::SensorVisionClient;
use crate::client::state::queries::GetStateSnapshot;
use crate::client::state::MqttScheme;

use crate::model::protocol::{CreateMetricPayload, CreateSensorRequest, DeleteMetricRequest, MetricValue, MetricsArrayRequest, PingRequest, PingResponse, PushMetricValueRequest, UpdateMetricRequest, UpdateSensorRequest};
use crate::model::sensor::Metric;
use crate::model::{MetricId, SensorId};

#[derive(Message)]
#[rtype(result = "Result<()>")]
pub struct PingTest;

#[derive(Message)]
#[rtype(result = "Result<()>")]
pub struct CreateSensor {
    pub name: String,
}

#[derive(Message)]
#[rtype(result = "Result<()>")]
pub struct UpdateSensor {
    pub sensor_id: SensorId,
    pub name: String,
    pub state: Option<bool>,
}

#[derive(Message)]
#[rtype(result = "Result<()>")]
pub struct DeleteSensor {
    pub sensor_id: SensorId,
}

#[derive(Message)]
#[rtype(result = "Result<String>")]
pub struct DumpSensors;

#[derive(Message)]
#[rtype(result = "Result<()>")]
pub struct LoadSensors;

#[derive(Message)]
#[rtype(result = "Result<()>")]
pub struct CreateMetrics {
    pub sensor_id: SensorId,
    pub metrics: Vec<Metric>,
}

#[derive(Message)]
#[rtype(result = "Result<()>")]
pub struct UpdateMetric {
    pub sensor_id: SensorId,
    pub metric_id: MetricId,
    pub name: Option<String>,
    pub value_annotation: Option<String>,
}

#[derive(Message)]
#[rtype(result = "Result<()>")]
pub struct DeleteMetric {
    pub sensor_id: SensorId,
    pub metric_id: MetricId,
}

#[derive(Message)]
#[rtype(result = "Result<()>")]
pub struct PushValue {
    pub sensor_id: SensorId,
    pub metric_id: MetricId,
    pub value: MetricValue,
    pub timestamp: Option<SystemTime>,
}

impl Handler<PingTest> for SensorVisionClient {
    type Result = ResponseFuture<Result<()>>;

    fn handle(&mut self, _: PingTest, _: &mut Self::Context) -> Self::Result {
        let request = PingRequest {
            request: String::from("Ping!"),
        };

        let mqtt_actor = self.mqtt_actor.clone();
        let connector_id = self.connector_id.clone();

        async move {
            let pong: PingResponse =
                Self::request_inner(&mqtt_actor, &connector_id, MqttScheme::Ping, &request).await?;

            if pong.answer == "Ping!" {
                Ok(())
            } else {
                Err(eyre!("Unexpected response: {:?}", pong))
            }
        }
        .boxed_local()
    }
}

impl Handler<CreateSensor> for SensorVisionClient {
    type Result = Result<()>;

    fn handle(
        &mut self,
        CreateSensor { name }: CreateSensor,
        _: &mut Self::Context,
    ) -> Self::Result {
        let request = CreateSensorRequest {
            name: String::from(name),
        };

        self.message(MqttScheme::SensorCreate, &request)
    }
}

impl Handler<UpdateSensor> for SensorVisionClient {
    type Result = Result<()>;

    fn handle(
        &mut self,
        UpdateSensor {
            sensor_id,
            name,
            state,
        }: UpdateSensor,
        _: &mut Self::Context,
    ) -> Self::Result {
        let request = UpdateSensorRequest {
            name: String::from(name),
            state: state.map(|x| x as u8),
        };
        self.message(MqttScheme::SensorUpdate(sensor_id), &request)
    }
}

impl Handler<DeleteSensor> for SensorVisionClient {
    type Result = Result<()>;

    fn handle(
        &mut self,
        DeleteSensor { sensor_id }: DeleteSensor,
        _: &mut Self::Context,
    ) -> Self::Result {
        Ok(self.raw_message(MqttScheme::SensorDelete(sensor_id), None))
    }
}

impl Handler<DumpSensors> for SensorVisionClient {
    type Result = ResponseFuture<Result<String>>;

    fn handle(&mut self, _: DumpSensors, _: &mut Self::Context) -> Self::Result {
        let state_actor = self.state_actor.clone();

        async move {
            let sensors = state_actor.send(GetStateSnapshot).await?;
            serde_json::to_string_pretty(&sensors).wrap_err("Failed to dump sensors")
        }
        .boxed_local()
    }
}

impl Handler<LoadSensors> for SensorVisionClient {
    type Result = Result<()>;

    fn handle(&mut self, _: LoadSensors, _: &mut Self::Context) -> Self::Result {
        Ok(self.raw_message(MqttScheme::SensorList, None))
    }
}

impl Handler<CreateMetrics> for SensorVisionClient {
    type Result = Result<()>;

    fn handle(
        &mut self,
        CreateMetrics { sensor_id, metrics }: CreateMetrics,
        _: &mut Self::Context,
    ) -> Self::Result {
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
}

impl Handler<UpdateMetric> for SensorVisionClient {
    type Result = Result<()>;

    fn handle(
        &mut self,
        UpdateMetric {
            sensor_id,
            metric_id,
            name,
            value_annotation,
        }: UpdateMetric,
        _: &mut Self::Context,
    ) -> Self::Result {
        let request = MetricsArrayRequest::one(UpdateMetricRequest {
            metric_id: metric_id.clone(),
            name,
            value_annotation,
        });
        self.message(MqttScheme::MetricUpdate(sensor_id), &request)
    }
}

impl Handler<DeleteMetric> for SensorVisionClient {
    type Result = Result<()>;

    fn handle(
        &mut self,
        DeleteMetric {
            sensor_id,
            metric_id,
        }: DeleteMetric,
        _: &mut Self::Context,
    ) -> Self::Result {
        let request = MetricsArrayRequest::one(DeleteMetricRequest {
            metric_id: metric_id.clone(),
        });
        self.message(MqttScheme::SensorDelete(sensor_id), &request)
    }
}

impl Handler<PushValue> for SensorVisionClient {
    type Result = Result<()>;

    fn handle(
        &mut self,
        PushValue {
            sensor_id,
            metric_id,
            value,
            timestamp,
        }: PushValue,
        _: &mut Self::Context,
    ) -> Self::Result {
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
}
