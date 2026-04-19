use crate::qadatastruct::mdsnapshot::{MDSnapshot, Tick};

/// L1 行情快照（股票，5档行情）—— 复用 [`MDSnapshot`]
pub type StockL1Snapshot = MDSnapshot;

/// 买一价
pub fn best_bid(snap: &MDSnapshot) -> f64 {
    snap.bid_price1
}

/// 卖一价
pub fn best_ask(snap: &MDSnapshot) -> f64 {
    snap.ask_price1
}

/// L1 快照转 Tick
pub fn l1_to_tick(snap: &MDSnapshot) -> Tick {
    snap.to_tick()
}

/// 是否涨停
pub fn is_limit_up(snap: &MDSnapshot) -> bool {
    snap.last_price >= snap.upper_limit
}

/// 是否跌停
pub fn is_limit_down(snap: &MDSnapshot) -> bool {
    snap.last_price <= snap.lower_limit
}
