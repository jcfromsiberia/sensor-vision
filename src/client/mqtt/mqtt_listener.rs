use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, StreamHandler, WeakRecipient};

use eyre::Result;

use futures::StreamExt;

use paho_mqtt as mqtt;

use crate::client::mqtt::{make_async_mqtt_client, MqttMessage};

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct MqttEvent(pub MqttMessage);

#[derive(Message)]
#[rtype(result = "()")]
pub struct SubscribeToListener(pub WeakRecipient<MqttEvent>);

pub struct MqttListenerService {
    mqtt_client: mqtt::AsyncClient,
    subscribers: Vec<WeakRecipient<MqttEvent>>,
}

impl MqttListenerService {
    pub async fn connect_and_start(topic: String) -> Result<Addr<Self>> {
        let (mqtt_client, conn_opts) = make_async_mqtt_client("sv_event")?;

        mqtt_client.connect(conn_opts).await?;
        mqtt_client.subscribe(&topic, mqtt::QOS_1).await?;

        Ok(Self {
            mqtt_client,
            subscribers: Vec::default(),
        }.start())
    }
}

impl Actor for MqttListenerService {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let message_stream = self.mqtt_client.get_stream(32);

        let event_stream = message_stream.filter_map(|msg_opt| async {
            msg_opt.map(|msg| MqttEvent(MqttMessage {
                topic: msg.topic().to_string(),
                message: String::from_utf8_lossy(msg.payload()).to_string(),
            }))
        });

        ctx.add_stream(event_stream);
    }
}

impl StreamHandler<MqttEvent> for MqttListenerService {
    fn handle(&mut self, item: MqttEvent, _: &mut Self::Context) {
        // Forward the message to all subscribers
        for subscriber in &self.subscribers {
            if let Some(subscriber) = subscriber.upgrade() {
                subscriber.do_send(item.clone());
            }
        }
    }
}

impl Handler<SubscribeToListener> for MqttListenerService {
    type Result = ();

    fn handle(&mut self, msg: SubscribeToListener, _: &mut Self::Context) -> Self::Result {
        self.subscribers.push(msg.0);
    }
}
