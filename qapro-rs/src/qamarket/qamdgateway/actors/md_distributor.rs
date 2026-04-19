#![allow(dead_code)]
use actix::prelude::*;
use hashbrown::{HashMap, HashSet};
use log::{debug, error, info};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

use crate::qadatastruct::mdsnapshot::MDSnapshot;
use crate::qamarket::qamdgateway::actors::messages::*;

/// 订阅者信息
struct Subscriber {
    addr: Recipient<MarketDataUpdateMessage>,
    instruments: HashSet<String>,
}

/// 市场数据分发器
///
/// 负责接收来自不同数据源的行情，按客户端订阅进行增量分发。
/// 支持全量首推 + 后续增量推送，减少不必要的带宽消耗。
pub struct MarketDataDistributor {
    subscribers: HashMap<String, Subscriber>,
    instrument_subscribers: HashMap<String, HashSet<String>>,
    market_data_cache: HashMap<String, MDSnapshot>,
    client_snapshots: HashMap<String, HashMap<String, MDSnapshot>>,
    last_batch_send: Instant,
    batch_interval: Duration,
}

impl Default for MarketDataDistributor {
    fn default() -> Self {
        Self::new()
    }
}

impl MarketDataDistributor {
    pub fn new() -> Self {
        Self {
            subscribers: HashMap::new(),
            instrument_subscribers: HashMap::new(),
            market_data_cache: HashMap::new(),
            client_snapshots: HashMap::new(),
            last_batch_send: Instant::now(),
            batch_interval: Duration::from_millis(100),
        }
    }

    fn snapshot_to_json(snap: &MDSnapshot) -> Value {
        serde_json::to_value(snap).unwrap_or_else(|_| json!({}))
    }

    /// 计算两个快照之间的差异字段（用于增量推送）
    fn diff_snapshots(old: &MDSnapshot, new: &MDSnapshot) -> Value {
        let old_v = serde_json::to_value(old).unwrap_or_default();
        let new_v = serde_json::to_value(new).unwrap_or_default();
        let mut diff = serde_json::Map::new();
        diff.insert("instrument_id".to_string(), json!(new.instrument_id));
        if let (Value::Object(o), Value::Object(n)) = (old_v, new_v) {
            for (k, v) in &n {
                if k == "instrument_id" {
                    continue;
                }
                if o.get(k) != Some(v) {
                    diff.insert(k.clone(), v.clone());
                }
            }
        }
        Value::Object(diff)
    }

    /// 向单个客户端发送行情（增量或全量）
    fn dispatch_to_client(&mut self, client_id: &str, instrument: &str, snap: &MDSnapshot) {
        let subscriber = match self.subscribers.get(client_id) {
            Some(s) => s,
            None => return,
        };
        if !subscriber.instruments.contains(instrument) {
            return;
        }
        let data_json = if let Some(client_cache) = self.client_snapshots.get(client_id) {
            if let Some(old) = client_cache.get(instrument) {
                let diff = Self::diff_snapshots(old, snap);
                if diff.as_object().map(|m| m.len() <= 1).unwrap_or(true) {
                    return; // 无变化
                }
                diff
            } else {
                Self::snapshot_to_json(snap)
            }
        } else {
            Self::snapshot_to_json(snap)
        };

        let mut data_map = HashMap::new();
        data_map.insert(instrument.to_string(), data_json.to_string());
        let msg = MarketDataUpdateMessage {
            instruments: vec![instrument.to_string()],
            data: data_map,
        };
        if let Err(e) = subscriber.addr.try_send(msg) {
            error!("Failed to send market data to {}: {}", client_id, e);
        } else {
            // 更新客户端快照缓存
            self.client_snapshots
                .entry(client_id.to_string())
                .or_default()
                .insert(instrument.to_string(), snap.clone());
        }
    }
}

impl Actor for MarketDataDistributor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("MarketDataDistributor started");
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        info!("MarketDataDistributor stopped");
    }
}

// ─── 处理行情更新 ────────────────────────────────────────────────

impl Handler<MarketDataUpdate> for MarketDataDistributor {
    type Result = ();

    fn handle(&mut self, msg: MarketDataUpdate, _ctx: &mut Self::Context) {
        let snap = msg.0;
        let instrument = snap.instrument_id.clone();

        // 更新全局缓存
        self.market_data_cache.insert(instrument.clone(), snap.clone());

        // 找到订阅了该合约的客户端
        let subscribers: Vec<String> = self
            .instrument_subscribers
            .get(&instrument)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();

        for client_id in subscribers {
            let snap_clone = snap.clone();
            self.dispatch_to_client(&client_id, &instrument, &snap_clone);
        }

        debug!("Dispatched {} to {} clients", instrument,
               self.instrument_subscribers.get(&instrument).map(|s| s.len()).unwrap_or(0));
    }
}

// ─── 处理订阅注册 ────────────────────────────────────────────────

impl Handler<RegisterDataReceiver> for MarketDataDistributor {
    type Result = ();

    fn handle(&mut self, msg: RegisterDataReceiver, _ctx: &mut Self::Context) {
        info!("Registering client: {}", msg.client_id);
        let mut instruments = HashSet::new();
        for inst in &msg.instruments {
            instruments.insert(inst.clone());
            self.instrument_subscribers
                .entry(inst.clone())
                .or_default()
                .insert(msg.client_id.clone());
        }
        self.subscribers.insert(
            msg.client_id.clone(),
            Subscriber {
                addr: msg.addr,
                instruments,
            },
        );

        // 为已订阅合约发送全量快照
        let cached: Vec<(String, MDSnapshot)> = msg
            .instruments
            .iter()
            .filter_map(|inst| {
                self.market_data_cache
                    .get(inst)
                    .map(|s| (inst.clone(), s.clone()))
            })
            .collect();
        for (inst, snap) in cached {
            self.dispatch_to_client(&msg.client_id, &inst, &snap);
        }
    }
}

impl Handler<UnregisterDataReceiver> for MarketDataDistributor {
    type Result = ();

    fn handle(&mut self, msg: UnregisterDataReceiver, _ctx: &mut Self::Context) {
        info!("Unregistering client: {}", msg.client_id);
        if let Some(sub) = self.subscribers.remove(&msg.client_id) {
            for inst in &sub.instruments {
                if let Some(set) = self.instrument_subscribers.get_mut(inst) {
                    set.remove(&msg.client_id);
                }
            }
        }
        self.client_snapshots.remove(&msg.client_id);
    }
}

impl Handler<UpdateSubscription> for MarketDataDistributor {
    type Result = ();

    fn handle(&mut self, msg: UpdateSubscription, _ctx: &mut Self::Context) {
        let client_id = msg.client_id.clone();
        let new_instruments: HashSet<String> = msg.instruments.into_iter().collect();

        if let Some(sub) = self.subscribers.get_mut(&client_id) {
            // 移除旧订阅
            for old in &sub.instruments {
                if !new_instruments.contains(old) {
                    if let Some(set) = self.instrument_subscribers.get_mut(old) {
                        set.remove(&client_id);
                    }
                }
            }
            // 添加新订阅
            for new in &new_instruments {
                if !sub.instruments.contains(new) {
                    self.instrument_subscribers
                        .entry(new.clone())
                        .or_default()
                        .insert(client_id.clone());
                }
            }
            sub.instruments = new_instruments.clone();
        }

        // 为新订阅合约推送全量
        let cached: Vec<(String, MDSnapshot)> = new_instruments
            .iter()
            .filter_map(|inst| {
                self.market_data_cache
                    .get(inst)
                    .map(|s| (inst.clone(), s.clone()))
            })
            .collect();
        for (inst, snap) in cached {
            self.dispatch_to_client(&client_id, &inst, &snap);
        }
    }
}

impl Handler<GetAllSubscriptions> for MarketDataDistributor {
    type Result = Vec<String>;

    fn handle(&mut self, _msg: GetAllSubscriptions, _ctx: &mut Self::Context) -> Vec<String> {
        self.instrument_subscribers.keys().cloned().collect()
    }
}
