#![allow(dead_code)]
use crate::qaprotocol::mifi::qafastkline::QAKlineBase;
use crate::qaruntime::base::{MQAddr, QAKline};
use crate::qaruntime::qamanagers::mq_manager::MQManager;
use actix::prelude::*;
use amiquip::{
    Connection, ConsumerMessage, ConsumerOptions, ExchangeDeclareOptions,
    ExchangeType, FieldTable, QueueDeclareOptions, Result,
};
use log::{error, info, warn};
use serde_json::Value;
type ParseResult<T> = std::result::Result<T, String>;

trait Attach {
    fn attach_init_data(&mut self, data: Value) -> ParseResult<()>;
    fn attach_update(&mut self, data: Value) -> ParseResult<()>;
}

impl Attach for QAKlineBase {
    fn attach_init_data(&mut self, data: Value) -> ParseResult<()> {
        let datetime = data["datetime"]
            .as_str()
            .ok_or_else(|| "missing datetime".to_string())?;
        let symbol = data["symbol"]
            .as_str()
            .ok_or_else(|| "missing symbol".to_string())?;
        let last_price = data["last_price"]
            .as_f64()
            .ok_or_else(|| "missing last_price".to_string())?;
        let volume = data["volume"]
            .as_f64()
            .ok_or_else(|| "missing volume".to_string())?;

        self.datetime = datetime.to_string();
        self.updatetime = datetime.to_string();
        self.code = symbol.to_string();
        self.open = last_price;
        self.high = last_price;
        self.low = last_price;
        self.close = last_price;
        self.volume = volume;
        Ok(())
    }

    fn attach_update(&mut self, data: Value) -> ParseResult<()> {
        if self.open == 0.0 {
            self.attach_init_data(data.clone())?;
        }
        let new_price = data["last_price"]
            .as_f64()
            .ok_or_else(|| "missing last_price".to_string())?;
        if self.high < new_price {
            self.high = new_price;
        }
        if self.low > new_price {
            self.low = new_price;
        }
        self.close = new_price;
        let cur_datetime = data["datetime"]
            .as_str()
            .ok_or_else(|| "missing datetime".to_string())?;
        self.updatetime = cur_datetime.to_string();
        Ok(())
    }
}

fn decode_delivery(body: Vec<u8>) -> Option<String> {
    String::from_utf8(body).map_err(|e| {
        error!("[MarketMQ] invalid UTF-8 payload: {}", e);
        e
    }).ok()
}

fn parse_kline_payload(data: &str) -> Option<QAKlineBase> {
    let kdata: Value = serde_json::from_str(data).map_err(|e| {
        error!("[MarketMQ] invalid JSON payload: {}", e);
        e
    }).ok()?;
    let mut kbar = QAKlineBase::init();
    kbar.attach_init_data(kdata).map_err(|e| {
        error!("[MarketMQ] invalid market payload: {}", e);
        e
    }).ok()?;
    Some(kbar)
}

// 订阅结构体
#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct Subscribe(pub Recipient<QAKline>);

#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct Start;

// MarketMQ 行情订阅与分发
pub struct MarketMQ {
    pub amqp: String,
    pub exchange: String,
    pub model: String,
    pub routing_key: String,
    // connection:
    pub subscribers: Vec<Recipient<QAKline>>,
    pub mqm: Addr<MQManager>,
}

impl MarketMQ {
    pub fn new(
        amqp: String,
        exchange: String,
        model: String,
        routing_key: String,
        mqm: Addr<MQManager>,
    ) -> Self {
        Self {
            amqp,
            exchange,
            model,
            routing_key,
            subscribers: Vec::new(),
            mqm,
        }
    }
    pub fn notify(&self, bar: QAKlineBase) {
        for subscr in &self.subscribers {
            match subscr.try_send(QAKline { data: bar.clone() }) {
                Err(e) => error!("[{}] notify fail {}", self.routing_key, e.to_string()),
                _ => {}
            }
        }
    }
    pub fn consume_direct(&self) -> Result<()> {
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
        info!("[{}] Receiving...", self.routing_key);

        queue.bind(&exchange, self.routing_key.clone(), FieldTable::new())?;

        let consumer = queue.consume(ConsumerOptions {
            no_ack: true,
            ..ConsumerOptions::default()
        })?;

        for (_i, message) in consumer.receiver().iter().enumerate() {
            match message {
                ConsumerMessage::Delivery(delivery) => {
                    let msg = delivery.body.clone();
                    if let Some(data) = decode_delivery(msg) {
                        if let Some(kbar) = parse_kline_payload(&data) {
                            // 未重采样Bar
                            self.notify(kbar.clone());
                        }
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

impl Actor for MarketMQ {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.set_mailbox_capacity(1000); // 设置邮箱容量
        match self.mqm.try_send(MQAddr {
            key: self.routing_key.clone(),
            addr: ctx.address().clone(),
        }) {
            Err(e) => error!("[{}] register fail {}", self.routing_key, e.to_string()),
            _ => {}
        }
    }
}

impl Handler<Subscribe> for MarketMQ {
    type Result = ();

    fn handle(&mut self, msg: Subscribe, _: &mut Self::Context) {
        self.subscribers.push(msg.0);
    }
}

impl Handler<Start> for MarketMQ {
    type Result = ();
    fn handle(&mut self, _msg: Start, _ctx: &mut Self::Context) -> Self::Result {
        let _ = self.consume_direct();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_kline_payload_rejects_missing_fields() {
        let payload = r#"{"datetime":"2026-04-19 09:30:00","symbol":"000001"}"#;
        assert!(parse_kline_payload(payload).is_none());
    }

    #[test]
    fn test_parse_kline_payload_accepts_valid_message() {
        let payload = r#"{"datetime":"2026-04-19 09:30:00","symbol":"000001","last_price":12.3,"volume":456}"#;
        let bar = parse_kline_payload(payload).expect("payload should parse");
        assert_eq!(bar.code, "000001");
        assert_eq!(bar.open, 12.3);
        assert_eq!(bar.volume, 456.0);
    }

    #[test]
    fn test_decode_delivery_rejects_invalid_utf8() {
        assert!(decode_delivery(vec![0xff, 0xfe]).is_none());
    }
}
