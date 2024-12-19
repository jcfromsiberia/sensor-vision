use eyre::{OptionExt, Result};

use log::LevelFilter;

use log4rs::append::file::FileAppender;
use log4rs::encode::pattern::PatternEncoder;
use log4rs::config::{Appender, Config, Logger, Root};

use std::fs;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;
use x509_certificate::X509Certificate;

use clap::{arg, command, ArgAction};

use uuid::Uuid;
use widgetui::App;

mod model;
mod client;
mod app;

use client::SensorVisionClient;
use client::mqtt::{setup_new_certificate, MqttClientWrapper};

use model::sensor::{Metric, ValueUnit, ValueType};
use model::protocol::MetricValue;
use crate::app::AppStateWrapper;
use crate::app::render::render_sensor_vision;

fn main() -> Result<()> {
    // TODO Add log rotation, see log4rs examples
    let logfile = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S)}\t[{l}]\t{P}/{T}\t{f}:{L}: {m}\n")))
        .build("sensor-vision.log")?;

    let config = Config::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .logger(Logger::builder()
            .appender("logfile")
            .additive(false)
            .build("sensor_vision", LevelFilter::Trace))
        .build(Root::builder()
            .appender("logfile")
            .build(LevelFilter::Error))?;

    log4rs::init_config(config)?;

    log::info!("Program Started");
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

    if matches.get_flag("test") {
        sleep(Duration::from_secs(1));
        let mut client = client_rc.lock().unwrap();
        client.create_sensor("Sensor19122024")?;
        // client.create_sensor("Sensor18122024")?;

        // let sensor_id = client.sensor_id_by_name("Sensor18122024").unwrap();
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
        // let metric_id = client.metric_id_by_name(&sensor_id, "CPUUsage").unwrap();
        //
        // client.push_value(&sensor_id, &metric_id, &MetricValue::Double(64f64), None)?;

        // client.update_metric(&sensor_id, &metric_id, None, Some("goods per minute"))?;
        // client.update_sensor(&sensor_id, "Sensor09122024", None)?;
        // client.delete_metric(&sensor_id, &metric_id)?;
        return Ok(());
    }

    // sleep(Duration::from_secs(1));

    log::info!("Sensor Dump:\n{}", client_rc.lock().unwrap().dump_sensors()?);

    let app_state = AppStateWrapper::new(&client_rc);
    App::new(100)?.states(app_state).widgets(render_sensor_vision).run()?;

    Ok(())
}
