use chrono::{DateTime, Local};
use serde_json::Value;
use std::collections::BTreeMap;
use actix_rt::{self, SystemRunner};
use std::{thread, time::Duration};

use crate::qaprotocol::qifi::account::QIFI;
use crate::qaconnector::mongo::mongoclient::QAMongoClient;
use crate::qaenv::localenv::CONFIG;

// TODO: 实盘WebSocket连接需要更新实现
// 原始代码使用 websocket crate 和 actix，当前仅保留核心数据结构

pub struct QATrader {
    pub qifi: QIFI,
    pub last_update_time: DateTime<Local>,
    pub runtime: SystemRunner,
    pub reconnect_attempts: u32,
    pub next_reconnect_delay_secs: u64,
    // TODO: ws_sender 需要更新 WebSocket 实现
    // pub ws_sender: Option<...>,
}

impl QATrader {
    pub fn new(
        account_cookie: String,
        password: String,
        wsuri: String,
        broker_name: String,
        portfolio: String,
        eventmq_ip: String,
        ping_gap: i32,
        bank_password: String,
        capital_password: String,
        taskid: String,
    ) -> Self {
        let mut qifi = QIFI::default();
        qifi.account_cookie = account_cookie;
        qifi.password = password;
        qifi.portfolio = portfolio;
        qifi.broker_name = broker_name;
        qifi.eventmq_ip = eventmq_ip;
        qifi.capital_password = capital_password;
        qifi.bank_password = bank_password;
        qifi.pub_host = "127.0.0.1".to_string();
        qifi.taskid = taskid;
        qifi.trade_host = "127.0.0.1".to_string();
        qifi.wsuri = wsuri;
        qifi.status = 200;
        qifi.ping_gap = ping_gap;

        Self {
            qifi,
            last_update_time: Local::now(),
            runtime: actix_rt::System::new(),
            reconnect_attempts: 0,
            next_reconnect_delay_secs: 1,
        }
    }

    pub fn register_disconnect(&mut self) -> u64 {
        self.reconnect_attempts += 1;
        let delay = self.next_reconnect_delay_secs.min(60);
        self.next_reconnect_delay_secs = delay.saturating_mul(2).min(60);
        delay
    }

    pub fn mark_reconnected(&mut self) {
        self.reconnect_attempts = 0;
        self.next_reconnect_delay_secs = 1;
    }

    pub fn parse(&mut self, msg: String) {
        if let Ok(val) = serde_json::from_str::<Value>(&msg) {
            if let Some(aid) = val["aid"].as_str() {
                let m = &val["data"][0]["trade"];
                let n = &val["data"][0]["notify"];
                match aid {
                    "rtn_data" if !m.is_null() => {
                        self.rtn_data_handler(m);
                    }
                    "rtn_data" if !n.is_null() => {
                        self.notify_handler(n);
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn rtn_data_handler(&mut self, data: &Value) {
        let account_cookie = self.qifi.account_cookie.clone();
        let new_message = &data[&account_cookie];

        if new_message.get("session").is_some() {
            if let Some(td) = new_message["session"]["trading_day"].as_str() {
                self.qifi.trading_day = td.to_string();
            }
        }

        if let Some(a) = new_message.get("accounts") {
            if let Some(cny) = a.get("CNY") {
                if let Ok(acc) = serde_json::from_value(cny.clone()) {
                    self.qifi.accounts = acc;
                }
            }
        }

        if let Some(investor_name) = new_message.get("investor_name") {
            if let Some(s) = investor_name.as_str() {
                self.qifi.investor_name = s.to_string();
            }
        }

        if let Some(orders) = new_message.get("orders") {
            if let Ok(new_ord) = serde_json::from_value::<BTreeMap<String, serde_json::Value>>(orders.clone()) {
                for (key, val) in new_ord {
                    if let Ok(order) = serde_json::from_value(val) {
                        self.qifi.orders.insert(key, order);
                    }
                }
            }
        }

        self.sync();
    }

    pub fn notify_handler(&mut self, data: &Value) {
        if let Some(ni) = data.as_object() {
            for (_k, v) in ni {
                if let Some(mess) = v["content"].as_str() {
                    self.qifi.event.insert(
                        Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                        mess.to_string(),
                    );
                    if mess.contains("用户登录失败") {
                        self.qifi.status = 600;
                    }
                }
            }
        }
        self.sync();
    }

    pub fn sync(&mut self) {
        self.last_update_time = Local::now();
        self.qifi.updatetime = self.last_update_time.format("%Y-%m-%d %H:%M:%S").to_string();
        let qifi = self.qifi.clone();
        let uri = CONFIG.account.uri.clone();
        for delay in [0_u64, 1, 2, 4] {
            if delay > 0 {
                thread::sleep(Duration::from_secs(delay));
            }
            let qifi_snapshot = qifi.clone();
            let uri_snapshot = uri.clone();
            let result = self.runtime.block_on(async move {
                let client = QAMongoClient::new(&uri_snapshot).await;
                client.save_qifi_slice(qifi_snapshot).await;
                Ok::<(), ()>(())
            });
            if result.is_ok() {
                return;
            }
        }
    }

    pub async fn sync_async(&mut self) {
        self.last_update_time = Local::now();
        self.qifi.updatetime = self.last_update_time.format("%Y-%m-%d %H:%M:%S").to_string();
        let client = QAMongoClient::new(&CONFIG.account.uri).await;
        client.save_qifi_slice(self.qifi.clone()).await;
    }
}
