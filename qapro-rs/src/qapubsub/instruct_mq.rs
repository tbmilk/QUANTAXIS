use crate::qaruntime::base::Instruct;
use crate::qaruntime::qamanagers::monitor_manager::MonitorManager;


use actix::{Actor, Addr, Context};
use amiquip::{
    Connection, ConsumerMessage, ConsumerOptions, ExchangeDeclareOptions,
    ExchangeType, FieldTable, QueueDeclareOptions, Result,
};
use log::{error, info, warn};

// 指令接收

fn decode_delivery(body: Vec<u8>) -> Option<String> {
    String::from_utf8(body).map_err(|e| {
        error!("[InstructMQ] invalid UTF-8 payload: {}", e);
        e
    }).ok()
}

pub struct InstructMQ {
    pub amqp: String,
    pub exchange: String,
    pub model: String,
    pub routing_key: String,
    // connection:
    pub morm: Addr<MonitorManager>,
}

impl InstructMQ {
    fn consume_direct(&self) -> Result<()> {
        let mut connection = Connection::insecure_open(&self.amqp)?;
        let channel = connection.open_channel(None)?;
        let exchange = channel.exchange_declare(
            ExchangeType::Direct,
            &self.exchange,
            ExchangeDeclareOptions {
                durable: false,
                auto_delete: false,
                internal: false,
                arguments: Default::default(),
            },
        )?;
        let queue = channel.queue_declare(
            "",
            QueueDeclareOptions {
                exclusive: true,
                ..QueueDeclareOptions::default()
            },
        )?;
        info!("[InstructMQ] Start at <{}> ", self.routing_key);

        queue.bind(&exchange, self.routing_key.clone(), FieldTable::new())?;

        let consumer = queue.consume(ConsumerOptions {
            no_ack: true,
            ..ConsumerOptions::default()
        })?;

        for (_i, message) in consumer.receiver().iter().enumerate() {
            match message {
                ConsumerMessage::Delivery(delivery) => {
                    let msg = delivery.body.clone();
                    let Some(data) = decode_delivery(msg) else {
                        continue;
                    };
                    match serde_json::from_str(&data) {
                        Ok(v) => match self.morm.try_send::<Instruct>(v) {
                            Ok(_) => {}
                            Err(e) => {
                                error!("[Monitor Manager] send instruct fail {}", e.to_string())
                            }
                        },
                        Err(e) => error!("[Monitor Manager] Instruct parse fail {}", e.to_string()),
                    }
                }
                other => {
                    warn!("Consumer ended: {:?}", other);
                    break;
                }
            }
        }
        connection.close()
    }
}

impl Actor for InstructMQ {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(1000); // 设置邮箱容量
        let _ = self.consume_direct();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_delivery_rejects_invalid_utf8() {
        assert!(decode_delivery(vec![0xff, 0xfe]).is_none());
    }

    #[test]
    fn test_decode_delivery_accepts_utf8() {
        assert_eq!(decode_delivery(br#"{"id":"1"}"#.to_vec()).as_deref(), Some(r#"{"id":"1"}"#));
    }
}
