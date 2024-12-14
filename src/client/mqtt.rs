use eyre::{eyre, OptionExt};

use futures::executor::block_on;
use futures::StreamExt;

use paho_mqtt as mqtt;

use sha2::{Digest, Sha256};

use signals2::{Connect2, Emit2};

use std::fs;
use std::time::Duration;

pub struct MqttClientWrapper {
    request_client: mqtt::AsyncClient,
    event_client: mqtt::AsyncClient,
    event_signal: signals2::Signal<(String, String)>,
}

impl MqttClientWrapper {
    pub fn new() -> eyre::Result<Self> {
        // TODO inject params
        log::trace!("Create MqttClientWrapper");
        let host = String::from("mqtts://localhost:18884");

        let trust_store =
            String::from("/var/lib/teamviewer-iot-agent/certs/TeamViewerAuthority.crt");
        let client_cert = String::from("clientCert.crt");
        let private_key = String::from("privkey.pem");

        let request_client = mqtt::CreateOptionsBuilder::new()
            .server_uri(&host)
            .max_buffered_messages(5)
            .client_id("sv_client")
            .create_client()?;

        let event_client = mqtt::CreateOptionsBuilder::new()
            .server_uri(&host)
            .max_buffered_messages(200)
            .client_id("sv_listener")
            .create_client()?;

        let ssl_opts = mqtt::SslOptionsBuilder::new()
            .trust_store(trust_store)?
            .key_store(client_cert)?
            .private_key(private_key)?
            .finalize();

        let conn_opts = mqtt::ConnectOptionsBuilder::new()
            .ssl_options(ssl_opts)
            .clean_session(false)
            .keep_alive_interval(Duration::from_secs(30))
            .finalize();

        log::debug!("Connecting to the broker");
        request_client.connect(conn_opts.clone()).wait()?;
        event_client.connect(conn_opts).wait()?;
        log::debug!("Connected");

        Ok(Self {
            request_client,
            event_client,
            event_signal: signals2::Signal::new(),
        })
    }

    pub fn sync_request(
        &mut self,
        topic: &str,
        response_topic: &str,
        error_topic: &str,
        payload: &str,
    ) -> eyre::Result<String> {
        log::trace!("Send SYNC Request\n\tTopic: {}\n\tResponse Topic: {}\n\tError Topic: {}\n\tMessage: {}",
            topic, response_topic, error_topic, payload);
        block_on(async {
            let client = &mut self.request_client;
            let mut stream = client.get_stream(2 << 14);

            let (topics, qos) = ([response_topic, error_topic], [mqtt::QOS_1, mqtt::QOS_1]);

            client.subscribe_many(&topics, &qos).await?;

            let message = mqtt::MessageBuilder::new()
                .topic(topic)
                .payload(payload.as_bytes())
                .qos(mqtt::QOS_1)
                .finalize();

            client.publish(message).await?;

            let optopt_message = stream.next().await;
            client.unsubscribe_many(&topics).await?;

            let message = optopt_message
                .ok_or_eyre("no payload")?
                .ok_or_eyre("no message")?;

            let payload = String::from_utf8_lossy(message.payload()).to_string();
            return if error_topic == message.topic() {
                Err(eyre!(payload))
            } else {
                Ok(payload)
            };
        })
    }

    pub fn async_message(&mut self, topic: &str, payload: &str) -> eyre::Result<()> {
        log::info!(
            "Send ASYNC Request\n\tTopic: {}\n\tMessage: {}",
            topic,
            payload
        );
        let message = mqtt::MessageBuilder::new()
            .topic(topic)
            .payload(payload.as_bytes())
            .qos(mqtt::QOS_1)
            .finalize();

        self.request_client.publish(message).wait()?;
        Ok(())
    }
    pub fn subscribe<Slot>(&mut self, topic: &str, slot: Slot) -> eyre::Result<signals2::Connection>
    where
        Slot: Fn(String, String) + Send + Sync + 'static,
    {
        log::trace!("Subscribe to MQTT topic(s) by {}", topic);
        if self.event_signal.count() == 1 {
            return Err(eyre!("Multiple subscribers are not supported"));
        }
        self.event_client.subscribe(topic, mqtt::QOS_1).wait()?;
        let weak_signal = self.event_signal.weak();
        self.event_client.set_message_callback(move |_, msg| {
            if let Some(message) = msg {
                let topic = message.topic().to_owned();
                let payload = String::from_utf8_lossy(message.payload()).to_string();
                weak_signal.upgrade().unwrap().emit(topic, payload);
            }
        });
        Ok(self.event_signal.connect(slot))
    }
}

pub fn setup_new_certificate() -> eyre::Result<()> {
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
