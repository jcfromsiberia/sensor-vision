use eyre::{eyre, OptionExt, Result};

use futures::executor::block_on;
use futures::StreamExt;

use paho_mqtt as mqtt;
use paho_mqtt::{AsyncClient, ConnectOptions};

use sha2::{Digest, Sha256};

use std::fs;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::sync::oneshot;

pub fn setup_new_certificate() -> Result<()> {
    // According to https://docs-iot.teamviewer.com/mqtt-api/#3-data-model
    log::trace!("Setup new client certificate");
    block_on(async {
        let csr_contents = fs::read("csr.pem").expect("Failed to read csr.pem");

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
                fs::write("clientCert.crt", message.payload())?;
            }
        }

        cli.disconnect(None).await?;
        Ok(())
    })
}

#[derive(Debug, Clone)]
pub struct MqttMessage {
    pub topic: String,
    pub message: String,
}

#[derive(Debug)]
pub enum MqttActorCommand {
    OneWayMessage {
        message: MqttMessage,
    },
    Request {
        request: MqttMessage,
        response_topic: String,
        error_topic: String,
        respond_to: oneshot::Sender<Result<String>>,
    },
}

struct MqttActor {
    receiver: mpsc::UnboundedReceiver<MqttActorCommand>,
    mqtt_client: AsyncClient,
}

impl MqttActor {
    fn new(receiver: mpsc::UnboundedReceiver<MqttActorCommand>, mqtt_client: AsyncClient) -> Self {
        Self {
            receiver,
            mqtt_client,
        }
    }

    fn handle_command(&mut self, command: MqttActorCommand) {
        use MqttActorCommand::*;
        match command {
            OneWayMessage {
                message,
            } => {
                let _ = self.message(message).expect("Failed to send");
            }
            Request {
                request,
                response_topic,
                error_topic,
                respond_to,
            } => {
                let result = self.request(request, &response_topic, &error_topic);
                let _ = respond_to.send(result);
            }
        }
    }

    fn message(&mut self, message: MqttMessage) -> Result<()> {
        let message = mqtt::MessageBuilder::new()
            .topic(&message.topic)
            .payload(message.message.as_bytes())
            .qos(mqtt::QOS_1)
            .finalize();

        self.mqtt_client.publish(message).wait()?;
        Ok(())
    }

    fn request(
        &mut self,
        message: MqttMessage,
        response_topic: &str,
        error_topic: &str,
    ) -> Result<String> {
        // Intentionally blocking to not interfere with concurrent request
        // topic subscriptions through this client
        // Alas this mqtt protocol is missing request-response cookies,
        // hence requests must be serialized.
        block_on(async {
            let client = &mut self.mqtt_client;
            let mut stream = client.get_stream(2 << 14);

            let (topics, qos) = ([response_topic, error_topic], [mqtt::QOS_1, mqtt::QOS_1]);
            client.subscribe_many(&topics, &qos).await?;

            let message = mqtt::MessageBuilder::new()
                .topic(&message.topic)
                .payload(message.message.as_bytes())
                .qos(mqtt::QOS_1)
                .finalize();

            client.publish(message).await?;

            let optopt_message = stream.next().await;
            client.unsubscribe_many(&topics).await?;

            let message = optopt_message
                .ok_or_eyre("no payload")?
                .ok_or_eyre("no message")?;

            let payload = String::from_utf8_lossy(message.payload()).to_string();

            if error_topic == message.topic() {
                Err(eyre!(payload))
            } else {
                Ok(payload)
            }
        })
    }
}

async fn mqtt_actor_loop(mut actor: MqttActor) {
    while let Some(command) = actor.receiver.recv().await {
        actor.handle_command(command);
    }
}

#[derive(Debug, Clone)]
pub struct MqttActorHandle {
    sender: mpsc::UnboundedSender<MqttActorCommand>,
}

impl MqttActorHandle {
    pub fn new() -> Result<Self> {
        let (client, conn_opts) = make_async_mqtt_client("sv_request")?;
        let (tx, rx) = mpsc::unbounded_channel::<MqttActorCommand>();

        // Blocking, wait for connect
        client.connect(conn_opts).wait()?;

        let actor = MqttActor::new(rx, client);

        tokio::task::Builder::new()
            .name("mqtt actor loop")
            .spawn(mqtt_actor_loop(actor))?;

        Ok(Self { sender: tx })
    }

    pub fn one_way_message(&self, message: MqttMessage) -> Result<()> {
        let command = MqttActorCommand::OneWayMessage {
            message,
        };
        Ok(self.sender.send(command)?)
    }

    pub async fn request(
        &self,
        request: MqttMessage,
        response_topic: String,
        error_topic: String,
    ) -> Result<String> {
        let (respond_to, receiver) = oneshot::channel();
        let command = MqttActorCommand::Request {
            request,
            response_topic,
            error_topic,
            respond_to,
        };
        self.sender.send(command)?;
        let result = receiver.await?;
        result
    }
}

async fn mqtt_listener_task(
    event_client: &mut mqtt::AsyncClient,
    sender: mpsc::UnboundedSender<MqttMessage>,
) -> Result<()> {
    let events_stream = event_client.get_stream(2 << 14);

    tokio::pin!(events_stream);

    while let Some(message) = events_stream.next().await {
        if let Some(message) = message {
            sender.send(MqttMessage {
                topic: message.topic().to_owned(),
                message: String::from_utf8_lossy(message.payload()).to_string(),
            })?;
        }
    }

    Ok(())
}

pub fn make_mqtt_listener(topic: String, client_name: String) -> Result<mpsc::UnboundedReceiver<MqttMessage>> {
    let (tx, rx) = mpsc::unbounded_channel::<MqttMessage>();
    let (mut event_client, conn_opts) = make_async_mqtt_client(&client_name)?;
    event_client.connect(conn_opts).wait()?;

    event_client.subscribe(&topic, mqtt::QOS_1).wait()?;

    tokio::task::Builder::new()
        .name("mqtt message listener")
        .spawn(async move {
            let _ = mqtt_listener_task(&mut event_client, tx).await;
            let _ = event_client.unsubscribe(&topic).await;
        })?;

    Ok(rx)
}

fn make_async_mqtt_client(client_name: &str) -> Result<(mqtt::AsyncClient, ConnectOptions)> {
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
