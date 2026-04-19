#![allow(dead_code)]
use actix::{Actor, ActorContext, AsyncContext, Handler, StreamHandler};
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use hashbrown::{HashMap, HashSet};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::qamarket::qamdgateway::actors::md_distributor::MarketDataDistributor;
use crate::qamarket::qamdgateway::actors::messages::*;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(30);

// ─── 客户端消息格式 ──────────────────────────────────────────────

/// 来自客户端的 WebSocket 消息（支持 TradingView 格式与传统格式）
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WsClientMessage {
    /// TradingView 格式订阅
    TvSubscribeQuote { aid: String, ins_list: String },
    /// Peek message
    PeekMessage { aid: String },
    /// 传统格式
    Legacy(LegacyClientMessage),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum LegacyClientMessage {
    #[serde(rename = "subscribe")]
    Subscribe { instruments: Vec<String> },
    #[serde(rename = "unsubscribe")]
    Unsubscribe { instruments: Vec<String> },
    #[serde(rename = "subscriptions")]
    Subscriptions,
    #[serde(rename = "ping")]
    Ping,
}

// ─── 服务端消息格式 ──────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WsServerMessage {
    /// TradingView 格式行情数据
    TvMarketData { aid: String, data: Vec<Value> },
    /// Peek 响应
    PeekResponse { aid: String, ins_list: String },
    /// 传统格式
    Legacy(LegacyServerMessage),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum LegacyServerMessage {
    #[serde(rename = "market_data")]
    MarketData { data: Value },
    #[serde(rename = "system")]
    System { message: String },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "subscriptions")]
    Subscriptions { instruments: Vec<String> },
    #[serde(rename = "pong")]
    Pong,
}

// ─── WebSocket 会话 ──────────────────────────────────────────────

/// 单个 WebSocket 客户端会话
pub struct WsSession {
    client_id: String,
    heartbeat: Instant,
    md_distributor: actix::Addr<MarketDataDistributor>,
    subscriptions: HashSet<String>,
    source: MarketDataSource,
}

impl WsSession {
    pub fn new(md_distributor: actix::Addr<MarketDataDistributor>, source: MarketDataSource) -> Self {
        Self {
            client_id: Uuid::new_v4().to_string(),
            heartbeat: Instant::now(),
            md_distributor,
            subscriptions: HashSet::new(),
            source,
        }
    }

    fn start_heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.heartbeat) > CLIENT_TIMEOUT {
                info!("Client {} heartbeat timeout, disconnecting", act.client_id);
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }

    fn parse_instruments(&self, ins_list: &str) -> Vec<String> {
        ins_list
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    fn handle_subscribe(&mut self, ctx: &mut ws::WebsocketContext<Self>, instruments: Vec<String>) {
        if instruments.is_empty() {
            self.send_error(ctx, "No instruments specified");
            return;
        }
        for inst in &instruments {
            self.subscriptions.insert(inst.clone());
        }
        self.md_distributor.do_send(UpdateSubscription {
            client_id: self.client_id.clone(),
            instruments: self.subscriptions.iter().cloned().collect(),
        });
        self.send_system(ctx, &format!("Subscribed to {} instruments", instruments.len()));
    }

    fn handle_unsubscribe(&mut self, ctx: &mut ws::WebsocketContext<Self>, instruments: Vec<String>) {
        for inst in &instruments {
            self.subscriptions.remove(inst);
        }
        self.md_distributor.do_send(UpdateSubscription {
            client_id: self.client_id.clone(),
            instruments: self.subscriptions.iter().cloned().collect(),
        });
        self.send_system(ctx, &format!("Unsubscribed from {} instruments", instruments.len()));
    }

    fn send_system(&self, ctx: &mut ws::WebsocketContext<Self>, message: &str) {
        let msg = WsServerMessage::Legacy(LegacyServerMessage::System {
            message: message.to_string(),
        });
        if let Ok(json) = serde_json::to_string(&msg) {
            ctx.text(json);
        }
    }

    fn send_error(&self, ctx: &mut ws::WebsocketContext<Self>, message: &str) {
        let msg = WsServerMessage::Legacy(LegacyServerMessage::Error {
            message: message.to_string(),
        });
        if let Ok(json) = serde_json::to_string(&msg) {
            ctx.text(json);
        }
    }
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.start_heartbeat(ctx);
        let addr = ctx.address();
        self.md_distributor.do_send(RegisterDataReceiver {
            client_id: self.client_id.clone(),
            addr: addr.recipient(),
            instruments: Vec::new(),
        });
        self.send_system(ctx, &format!("Connected. Session: {}", self.client_id));
    }

    fn stopping(&mut self, _: &mut Self::Context) -> actix::Running {
        self.md_distributor.do_send(UnregisterDataReceiver {
            client_id: self.client_id.clone(),
        });
        actix::Running::Stop
    }
}

// ─── 处理来自 WebSocket 的消息 ───────────────────────────────────

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(m)) => {
                self.heartbeat = Instant::now();
                ctx.pong(&m);
            }
            Ok(ws::Message::Pong(_)) => {
                self.heartbeat = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                self.heartbeat = Instant::now();
                match serde_json::from_str::<WsClientMessage>(&text) {
                    Ok(WsClientMessage::TvSubscribeQuote { aid, ins_list })
                        if aid == "subscribe_quote" =>
                    {
                        let instruments = self.parse_instruments(&ins_list);
                        self.handle_subscribe(ctx, instruments);
                        let resp = WsServerMessage::PeekResponse {
                            aid: "rsp_subscribe_quote".to_string(),
                            ins_list,
                        };
                        if let Ok(j) = serde_json::to_string(&resp) {
                            ctx.text(j);
                        }
                    }
                    Ok(WsClientMessage::PeekMessage { aid }) if aid == "peek_message" => {
                        let ins_list = self.subscriptions.iter().cloned().collect::<Vec<_>>().join(",");
                        let resp = WsServerMessage::PeekResponse {
                            aid: "rsp_peek_message".to_string(),
                            ins_list,
                        };
                        if let Ok(j) = serde_json::to_string(&resp) {
                            ctx.text(j);
                        }
                    }
                    Ok(WsClientMessage::Legacy(m)) => match m {
                        LegacyClientMessage::Subscribe { instruments } => {
                            self.handle_subscribe(ctx, instruments);
                        }
                        LegacyClientMessage::Unsubscribe { instruments } => {
                            self.handle_unsubscribe(ctx, instruments);
                        }
                        LegacyClientMessage::Subscriptions => {
                            let list: Vec<String> =
                                self.subscriptions.iter().cloned().collect();
                            let msg = WsServerMessage::Legacy(LegacyServerMessage::Subscriptions {
                                instruments: list,
                            });
                            if let Ok(j) = serde_json::to_string(&msg) {
                                ctx.text(j);
                            }
                        }
                        LegacyClientMessage::Ping => {
                            let msg =
                                WsServerMessage::Legacy(LegacyServerMessage::Pong);
                            if let Ok(j) = serde_json::to_string(&msg) {
                                ctx.text(j);
                            }
                        }
                    },
                    Err(e) => {
                        error!("Failed to parse WebSocket message: {}", e);
                        self.send_error(ctx, &format!("Invalid message format: {}", e));
                    }
                    _ => {
                        warn!("Unknown message type: {}", text);
                    }
                }
            }
            Ok(ws::Message::Close(reason)) => {
                info!("WebSocket closed: {:?}", reason);
                ctx.close(reason);
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

// ─── 处理来自分发器的行情推送 ────────────────────────────────────

impl Handler<MarketDataUpdateMessage> for WsSession {
    type Result = ();

    fn handle(&mut self, msg: MarketDataUpdateMessage, ctx: &mut Self::Context) {
        for inst in &msg.instruments {
            if self.subscriptions.contains(inst) {
                if let Some(data_json) = msg.data.get(inst) {
                    if let Ok(data_value) = serde_json::from_str::<Value>(data_json) {
                        let mut quotes = HashMap::new();
                        if let Some(id) =
                            data_value.get("instrument_id").and_then(|v| v.as_str())
                        {
                            quotes.insert(id.to_string(), data_value.clone());
                            let tv_data = json!({
                                "aid": "rtn_data",
                                "data": [{"quotes": quotes}]
                            });
                            if let Ok(s) = serde_json::to_string(&tv_data) {
                                ctx.text(s);
                                debug!("Sent update for {} to {}", inst, self.client_id);
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── HTTP 升级处理函数 ────────────────────────────────────────────

/// Actix-web 路由处理：将 HTTP 连接升级为 WebSocket
pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    distributor: web::Data<actix::Addr<MarketDataDistributor>>,
) -> Result<HttpResponse, Error> {
    let query = req.query_string();
    let source = if query.contains("source=qq") {
        MarketDataSource::QQ
    } else if query.contains("source=sina") {
        MarketDataSource::Sina
    } else {
        MarketDataSource::CTP
    };
    let session = WsSession::new(distributor.get_ref().clone(), source);
    ws::start(session, &req, stream)
}
