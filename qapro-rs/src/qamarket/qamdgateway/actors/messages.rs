use actix::prelude::*;
use hashbrown::HashMap;

use crate::qadatastruct::mdsnapshot::MDSnapshot;
use crate::qamarket::qamdgateway::actors::md_distributor::MarketDataDistributor;

/// 市场数据来源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarketDataSource {
    /// CTP 期货接口
    CTP,
    /// 腾讯行情
    QQ,
    /// 新浪行情
    Sina,
    /// 自定义/其他
    Custom,
}

// ─── 通用控制消息 ───────────────────────────────────────────────

#[derive(Message)]
#[rtype(result = "()")]
pub struct InitMarketDataSource;

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct LoginMarketDataSource;

#[derive(Message)]
#[rtype(result = "()")]
pub struct StartMarketData {
    pub instruments: Vec<String>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct StopMarketData;

#[derive(Message)]
#[rtype(result = "()")]
pub struct RestartActor;

// ─── 分发器注册消息 ─────────────────────────────────────────────

#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterDistributor {
    pub addr: Addr<MarketDataDistributor>,
}

/// 注册 WebSocket 数据接收者
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterDataReceiver {
    pub client_id: String,
    pub addr: Recipient<MarketDataUpdateMessage>,
    pub instruments: Vec<String>,
}

/// 取消注册接收者
#[derive(Message)]
#[rtype(result = "()")]
pub struct UnregisterDataReceiver {
    pub client_id: String,
}

/// 更新客户端订阅列表
#[derive(Message)]
#[rtype(result = "()")]
pub struct UpdateSubscription {
    pub client_id: String,
    pub instruments: Vec<String>,
}

/// 查询客户端订阅列表
#[derive(Message)]
#[rtype(result = "Vec<String>")]
pub struct QuerySubscription {
    pub client_id: String,
}

/// 查询全部订阅
#[derive(Message)]
#[rtype(result = "Vec<String>")]
pub struct GetAllSubscriptions;

// ─── 行情推送消息 ────────────────────────────────────────────────

/// 从数据源到分发器的原始行情更新（通用，带来源标记）
#[derive(Message, Clone)]
#[rtype(result = "()")]
pub struct MarketDataUpdate(pub MDSnapshot, pub MarketDataSource);

/// 分发器推送给 WebSocket 客户端的增量行情
#[derive(Message, Clone)]
#[rtype(result = "()")]
pub struct MarketDataUpdateMessage {
    pub instruments: Vec<String>,
    pub data: HashMap<String, String>,
}

// ─── WebSocket 连接消息 ──────────────────────────────────────────

#[derive(Message)]
#[rtype(result = "()")]
pub struct WSMessage(pub String);

#[derive(Message)]
#[rtype(result = "()")]
pub struct Connect {
    pub addr: Recipient<WSMessage>,
    pub client_id: String,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    pub client_id: String,
}
