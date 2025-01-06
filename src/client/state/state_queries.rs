use actix::{Handler, Message, MessageResult};

use std::collections::HashSet;

use crate::client::state::{Sensors, SensorsStateActor};
use crate::model::{MetricId, SensorId};

#[derive(Message)]
#[rtype(result = "Sensors")]
pub struct GetStateSnapshot;

#[derive(Message)]
#[rtype(result = "Option<HashSet<MetricId>>")]
pub struct GetMetricIds(pub SensorId);

#[derive(Message)]
#[rtype(result = "Option<SensorId>")]
pub struct GetSensorIdByName(pub String);

#[derive(Message)]
#[rtype(result = "Option<MetricId>")]
pub struct GetMetricIdByName(pub SensorId, pub String);

impl Handler<GetStateSnapshot> for SensorsStateActor {
    type Result = MessageResult<GetStateSnapshot>;

    fn handle(&mut self, _: GetStateSnapshot, _: &mut Self::Context) -> Self::Result {
        MessageResult(self.sensors.clone())
    }
}

impl Handler<GetMetricIds> for SensorsStateActor {
    type Result = Option<HashSet<MetricId>>;

    fn handle(
        &mut self,
        GetMetricIds(sensor_id): GetMetricIds,
        _: &mut Self::Context,
    ) -> Self::Result {
        self.sensors.get(&sensor_id).map(|sensor| {
            sensor
                .metrics
                .iter()
                .map(|m| m.metric_id().clone())
                .collect()
        })
    }
}

impl Handler<GetSensorIdByName> for SensorsStateActor {
    type Result = Option<SensorId>;

    fn handle(
        &mut self,
        GetSensorIdByName(sensor_name): GetSensorIdByName,
        _: &mut Self::Context,
    ) -> Self::Result {
        self.sensors
            .iter()
            .find(|(_, sensor)| sensor.name == sensor_name)
            .map(|(sensor_id, _)| *sensor_id)
    }
}

impl Handler<GetMetricIdByName> for SensorsStateActor {
    type Result = Option<MetricId>;

    fn handle(
        &mut self,
        GetMetricIdByName(sensor_id, name): GetMetricIdByName,
        _: &mut Self::Context,
    ) -> Self::Result {
        self.sensors
            .get(&sensor_id)
            .map(|sensor| {
                sensor
                    .metrics
                    .iter()
                    .find(|metric| metric.name() == &name)
                    .map(|metric| metric.metric_id().clone())
            })
            .flatten()
    }
}
