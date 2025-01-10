use actix::{
    Actor, Addr, AsyncContext, Context, Handler, WrapFuture,
};

use eyre::Result;

use futures::FutureExt;

use serde::{Deserialize, Serialize};

use crate::client::mqtt::{
    MqttActor, MqttListenerService, MqttMessage, MqttRequest, OneWayMessage, SubscribeToListener,
};
use crate::client::state::queries::{
    GetMetricIdByName, GetMetricIds, GetSensorIdByName, GetStateSnapshot,
};
use crate::client::state::{
    queries, MqttScheme, SensorStateEvent, SensorsStateActor, SubscribeToStateEvents,
};
use crate::model::{ConnectorId};

#[derive(Clone)]
pub struct SensorVisionClient {
    pub(crate) connector_id: ConnectorId,

    pub(crate) mqtt_actor: Addr<MqttActor>,
    pub(crate) state_actor: Addr<SensorsStateActor>,

    #[allow(dead_code)]
    mqtt_listener_service: Addr<MqttListenerService>,
}

impl SensorVisionClient {
    pub async fn new(connector_id: ConnectorId) -> Result<Self> {
        let events_topic = format!("/v1.0/{}/#", connector_id);
        let mqtt_actor = MqttActor::connect_and_start().await?;
        let mqtt_listener_service = MqttListenerService::connect_and_start(events_topic).await?;
        let state_actor = SensorsStateActor::new().start();

        mqtt_listener_service
            .send(SubscribeToListener(state_actor.downgrade().recipient()))
            .await?;

        Ok(Self {
            connector_id,
            mqtt_actor,
            state_actor,
            mqtt_listener_service,
        })
    }

    pub(crate) fn raw_message_inner(
        mqtt_actor: &Addr<MqttActor>,
        connector_id: &ConnectorId,
        scheme: MqttScheme,
        payload: Option<String>,
    ) {
        let (topic, _, _) = scheme.get_topics();
        let full_topic = format!("/v1.0/{}/{}", connector_id, topic);

        let message = payload.unwrap_or(String::from("{}"));

        mqtt_actor.do_send(OneWayMessage(MqttMessage {
            topic: full_topic,
            message,
        }));
    }

    pub(crate) fn raw_message(&self, scheme: MqttScheme, payload: Option<String>) {
        Self::raw_message_inner(&self.mqtt_actor, &self.connector_id, scheme, payload);
    }

    pub(crate) fn message<Blueprint: Serialize>(
        &self,
        scheme: MqttScheme,
        body: &Blueprint,
    ) -> Result<()> {
        let body_serialized = serde_json::to_string(body)?;
        self.raw_message(scheme, Some(body_serialized));
        Ok(())
    }

    pub(crate) async fn raw_request_inner(
        mqtt_actor: &Addr<MqttActor>,
        connector_id: &ConnectorId,
        scheme: MqttScheme,
        message: Option<String>,
    ) -> Result<String> {
        let (topic, response_topic, error_topic) = scheme.get_topics();

        let full_topic = format!("/v1.0/{}/{}", connector_id, topic);
        let full_response_topic = format!("/v1.0/{}/{}", connector_id, response_topic);
        let full_error_topic = format!("/v1.0/{}/{}", connector_id, error_topic);

        let message = message.unwrap_or(String::from("{}"));

        Ok(mqtt_actor
            .send(MqttRequest {
                message: MqttMessage {
                    topic: full_topic,
                    message,
                },
                response_topic: full_response_topic,
                error_topic: full_error_topic,
            })
            .await??)
    }

    #[allow(dead_code)]
    pub(crate) async fn raw_request(
        &self,
        scheme: MqttScheme,
        message: Option<String>,
    ) -> Result<String> {
        Self::raw_request_inner(&self.mqtt_actor, &self.connector_id, scheme, message).await
    }

    pub(crate) async fn request_inner<Request: Serialize, Response: for<'a> Deserialize<'a>>(
        mqtt_actor: &Addr<MqttActor>,
        connector_id: &ConnectorId,
        scheme: MqttScheme,
        request: &Request,
    ) -> Result<Response> {
        let request_serialized = serde_json::to_string(request)?;
        let response_serialized =
            Self::raw_request_inner(mqtt_actor, connector_id, scheme, Some(request_serialized))
                .await?;
        Ok(serde_json::from_str(&response_serialized)?)
    }

    #[allow(dead_code)]
    pub(crate) async fn request<Request: Serialize, Response: for<'a> Deserialize<'a>>(
        &self,
        scheme: MqttScheme,
        request: &Request,
    ) -> Result<Response> {
        Self::request_inner(&self.mqtt_actor, &self.connector_id, scheme, request).await
    }
}

impl Actor for SensorVisionClient {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let state_actor = self.state_actor.clone();
        let weak_this = ctx.address().downgrade().recipient();
        ctx.spawn(
            async move {
                let _ = state_actor.send(SubscribeToStateEvents(weak_this)).await;
            }
            .into_actor(self),
        );
    }
}

impl Handler<SensorStateEvent> for SensorVisionClient {
    type Result = ();

    fn handle(&mut self, event: SensorStateEvent, ctx: &mut Self::Context) -> Self::Result {
        use SensorStateEvent::*;
        match &event {
            NewLinkedSensorLoaded(linked_sensor) | ExistingLinkedSensorLoaded(linked_sensor) => {
                for linked_metric in &linked_sensor.metrics {
                    self.raw_message(
                        MqttScheme::MetricDescribe(
                            linked_sensor.sensor_id,
                            linked_metric.metric_id,
                        ),
                        None,
                    );
                }
            }

            NewMetricCreated {
                sensor_id,
                metric_id,
            } => self.raw_message(MqttScheme::MetricDescribe(*sensor_id, *metric_id), None),

            SensorUpdated { .. } => {
                // There is no other way to get sensor/metric update details
                // rather than reloading all the sensors again :(
                self.raw_message(MqttScheme::SensorList, None)
            }

            SensorMetricsUpdated { sensor_id } => {
                let query = queries::GetMetricIds(*sensor_id);
                let state_actor = self.state_actor.clone();
                let mqtt_actor = self.mqtt_actor.clone();
                let connector_id = self.connector_id.clone();
                let sensor_id = *sensor_id;
                ctx.spawn(
                    async move {
                        let query_result = state_actor.send(query).await;
                        if let Err(err) = query_result {
                            log::error!("Query failed: {}", err);
                        } else {
                            if let Some(metric_ids) = query_result.unwrap() {
                                for metric_id in metric_ids {
                                    Self::raw_message_inner(
                                        &mqtt_actor,
                                        &connector_id,
                                        MqttScheme::MetricDescribe(sensor_id.clone(), metric_id),
                                        None,
                                    );
                                }
                            }
                        }
                    }
                    .into_actor(self),
                );
            }

            _ => {}
        }
    }
}

macro_rules! delegate_state_queries {
    ($actor:ty, { $( $msg:ty ),* $(,)? }) => {
        $(
            impl actix::Handler<$msg> for $actor {
                type Result = actix::ResponseFuture<<$msg as actix::Message>::Result>;

                fn handle(&mut self, msg: $msg, _: &mut Self::Context) -> Self::Result {
                    let state_actor = self.state_actor.clone();
                    async move {
                        state_actor.send(msg).await.expect("Delegating failed")
                    }.boxed_local()
                }
            }
        )*
    };
}

delegate_state_queries!(SensorVisionClient, {
    SubscribeToStateEvents,
    GetStateSnapshot,
    GetMetricIds,
    GetSensorIdByName,
    GetMetricIdByName,
});
