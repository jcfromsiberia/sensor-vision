use eyre::{OptionExt, Result};


use std::fs;
use std::sync::{Arc, Mutex};

use x509_certificate::X509Certificate;

use clap::{arg, command, ArgAction};
use uuid::Uuid;

use sensor_vision::client::mqtt::MqttActorHandle;
use sensor_vision::client::client::SensorVisionClient;
use sensor_vision::model::sensor::{Metric, ValueUnit, ValueType};
use sensor_vision::model::protocol::MetricValue;

use tokio::time::{sleep, Duration};
use sensor_vision::model::ConnectorId;

#[tokio::main]
async fn main() -> Result<()> {
    let cert_contents = fs::read("clientCert.crt")?;

    let cert = X509Certificate::from_pem(&cert_contents).expect("Failed to parse clientCert.crt");

    let connector_id = cert.subject_common_name().ok_or_eyre("Certificate has no CN")?;
    let connector_id: ConnectorId = connector_id.into();

    let sv_client = SensorVisionClient::new(connector_id)?;

    sv_client.ping_test().await?;

    sv_client.load_sensors().await?;

    sleep(Duration::from_secs(3)).await;

    // sv_client.create_sensor("Sensor19122024").await?;

    // sleep(Duration::from_secs(1)).await;
    //
    let sensor_id = sv_client.sensor_id_by_name("Sensor19122024").await.unwrap();
    //
    // let metrics = vec![
    //     Metric::predefined(
    //         String::from("CPUUsage"),
    //         ValueUnit::Percent,
    //     ),
    //     Metric::predefined(
    //         String::from("CPUIdle"),
    //         ValueUnit::Percent,
    //     ),
    //     Metric::custom(
    //         String::from("ShopFloor1"),
    //         ValueType::Integer,
    //         String::from("goods per minute"),
    //     ),
    // ];
    // sv_client.create_metrics(sensor_id, &metrics).await?;
    // sleep(Duration::from_secs(1)).await;
    let metric_id = sv_client.metric_id_by_name(sensor_id, "CPUUsage").await.unwrap();

    sv_client.push_value(sensor_id, metric_id, MetricValue::Double(64f64), None).await?;

    // sv_client.update_metric(&sensor_id, &metric_id, None, Some("goods per minute"))?;
    // sv_client.update_sensor(&sensor_id, "Sensor09122024", None)?;
    // sv_client.delete_metric(&sensor_id, &metric_id)?;

    Ok(())
}
