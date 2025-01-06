use actix::prelude::*;

use eyre::{eyre, OptionExt, Result};

use futures::{FutureExt, StreamExt};

use paho_mqtt as mqtt;

use sha2::{Digest, Sha256};

use std::time::Duration;

#[derive(Debug, Clone)]
pub struct MqttMessage {
    pub topic: String,
    pub message: String,
}

#[derive(Message)]
#[rtype(result = "Result<()>")]
pub struct OneWayMessage(pub MqttMessage);

#[derive(Message)]
#[rtype(result = "Result<String>")]
pub struct MqttRequest {
    pub message: MqttMessage,
    pub response_topic: String,
    pub error_topic: String,
}

pub struct MqttActor {
    mqtt_client: mqtt::AsyncClient,
}

impl MqttActor {
    pub async fn connect_and_start() -> Result<Addr<Self>> {
        let (mqtt_client, connect_opts) = make_async_mqtt_client("sv_client")?;

        mqtt_client.connect(connect_opts).await?;

        Ok(Self { mqtt_client }.start())
    }
}

impl Actor for MqttActor {
    type Context = Context<Self>;
}

impl Handler<OneWayMessage> for MqttActor {
    type Result = ResponseFuture<Result<()>>;

    fn handle(&mut self, OneWayMessage(msg): OneWayMessage, _: &mut Self::Context) -> Self::Result {
        let message = mqtt::MessageBuilder::new()
            .topic(&msg.topic)
            .payload(msg.message.as_bytes())
            .qos(mqtt::QOS_1)
            .finalize();

        let mqtt_client = self.mqtt_client.clone();
        async move {
            Ok(mqtt_client.publish(message).await?)
        }.boxed_local()
    }
}

impl Handler<MqttRequest> for MqttActor {
    type Result = ResponseFuture<Result<String>>;

    fn handle(&mut self, msg: MqttRequest, _: &mut Self::Context) -> Self::Result {
        let mut client = self.mqtt_client.clone();
        async move {
            let mut stream = client.get_stream(2 << 14);
            let (topics, qos) = (
                [&msg.response_topic, &msg.error_topic],
                [mqtt::QOS_1, mqtt::QOS_1],
            );
            client.subscribe_many(&topics, &qos).await?;
            let message = mqtt::MessageBuilder::new()
                .topic(&msg.message.topic)
                .payload(msg.message.message.as_bytes())
                .qos(mqtt::QOS_1)
                .finalize();
            client.publish(message).await?;
            let optopt_message = stream.next().await;
            client.unsubscribe_many(&topics).await?;
            let message = optopt_message
                .ok_or_eyre("no payload")?
                .ok_or_eyre("no message")?;

            let payload = String::from_utf8_lossy(message.payload()).to_string();

            if msg.error_topic == message.topic() {
                Err(eyre!(payload))
            } else {
                Ok(payload)
            }
        }.boxed_local()
    }
}

pub fn make_async_mqtt_client(client_name: &str) -> Result<(mqtt::AsyncClient, mqtt::ConnectOptions)> {
    let host = String::from("mqtts://localhost:18884");

    let trust_store = String::from("/var/lib/teamviewer-iot-agent/certs/TeamViewerAuthority.crt");
    let client_cert = String::from("clientCert.crt");
    let private_key = String::from("privkey.pem");

    let async_client = mqtt::CreateOptionsBuilder::new()
        .server_uri(&host)
        .max_buffered_messages(200)
        .client_id(client_name)
        .create_client()?;

    let ssl_opts = mqtt::SslOptionsBuilder::new()
        .trust_store(trust_store)?
        .key_store(client_cert)?
        .private_key(private_key)?
        .finalize();

    let conn_opts = mqtt::ConnectOptionsBuilder::new()
        .ssl_options(ssl_opts)
        .clean_session(false)
        .keep_alive_interval(Duration::from_secs(120))
        .finalize();

    Ok((async_client, conn_opts))
}

pub async fn setup_new_certificate() -> Result<()> {
    // According to https://docs-iot.teamviewer.com/mqtt-api/#3-data-model
    let csr_contents = std::fs::read("csr.pem").expect("Failed to read csr.pem");

    let mut hasher = Sha256::new();
    hasher.update(&csr_contents);
    let csr_digest = hasher.finalize();

    let certback_topic = format!("/certBack/{:x}", csr_digest);

    let trust_store =
        String::from("/var/lib/teamviewer-iot-agent/certs/TeamViewerAuthority.crt");

    let host = String::from("mqtts://localhost:18883");
    let mut cli = mqtt::CreateOptionsBuilder::new()
        .server_uri(&host)
        .max_buffered_messages(100)
        .client_id("sv_cert")
        .create_client()?;

    let ssl_opts = mqtt::SslOptionsBuilder::new()
        .trust_store(trust_store)?
        .finalize();

    let conn_opts = mqtt::ConnectOptionsBuilder::new()
        .ssl_options(ssl_opts)
        .clean_session(false)
        .keep_alive_interval(Duration::from_secs(30))
        .finalize();

    let mut strm = cli.get_stream(25);

    cli.connect(conn_opts).await?;

    cli.subscribe(certback_topic, mqtt::QOS_1).await?;

    let cert_request_msg = mqtt::MessageBuilder::new()
        .topic("/v1.0/createClient")
        .payload(csr_contents)
        .qos(mqtt::QOS_1)
        .finalize();

    cli.publish(cert_request_msg).await?;

    if let Some(message_opt) = strm.next().await {
        if let Some(message) = message_opt {
            std::fs::write("clientCert.crt", message.payload())?;
        }
    }

    cli.disconnect(None).await?;
    Ok(())
}
