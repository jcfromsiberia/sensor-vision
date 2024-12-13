use eyre::{OptionExt, Result};

use std::fs;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;
use x509_certificate::X509Certificate;

use clap::{arg, command, ArgAction};

use uuid::Uuid;

mod model;
mod client;

use client::SensorVisionClient;
use client::mqtt::{setup_new_certificate, MqttClientWrapper};

use model::sensor::{Metric, ValueUnit, ValueType};
use model::protocol::MetricValue;

fn main() -> Result<()> {
    let matches = command!()
        .arg(arg!(-n --new "Quick setup a new connector").action(ArgAction::SetTrue))
        .arg(arg!(-t --test "Generate test sensor and metrics").action(ArgAction::SetTrue))
        .arg(arg!(-p --ping "Ping test").action(ArgAction::SetTrue))
        .get_matches();

    if matches.get_flag("new") {
        return setup_new_certificate();
    }

    let cert_contents = fs::read("clientCert.crt")?;

    let cert = X509Certificate::from_pem(&cert_contents).expect("Failed to parse clientCert.crt");

    let connector_id = cert.subject_common_name().ok_or_eyre("Certificate has no CN")?;
    let connector_id = Uuid::parse_str(&connector_id)?;
    let mqtt_client = Arc::new(Mutex::new(MqttClientWrapper::new()?));

    let client_rc = SensorVisionClient::new(connector_id, mqtt_client)?;
    {
        let mut client = client_rc.lock().unwrap();

        if matches.get_flag("ping") {
            return client.ping_test();
        }

        client.load_sensors()?;
    }
    // Wait while sensors being loaded
    sleep(Duration::from_secs(1));

    if matches.get_flag("test") {
        let mut client = client_rc.lock().unwrap();
        // client.create_sensor("Sensor08122024")?;

        let sensor_id = client.sensor_id_by_name("Sensor08122024").unwrap();
        // client.delete_sensor(&sensor_id)?;

        // {
        //     let metric_id = client.metric_id_by_name(&sensor_id, "CPUUsage").unwrap();
        //     client.delete_metric(&sensor_id, &metric_id)?;
        // }

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
        // client.create_metrics(&sensor_id, &metrics)?;
        let metric_id = client.metric_id_by_name(&sensor_id, "CPUUsage").unwrap();
        //
        client.push_value(&sensor_id, &metric_id, &MetricValue::Double(64f64), None)?;

        // client.update_metric(&sensor_id, &metric_id, None, Some("goods per minute"))?;
        // client.update_sensor(&sensor_id, "Sensor09122024", None)?;
        // client.delete_metric(&sensor_id, &metric_id)?;
    }

    sleep(Duration::from_secs(1));

    println!("{}", client_rc.lock().unwrap().dump_sensors()?);

    Ok(())
}
