use crate::qadatastruct::mdsnapshot::MDSnapshot;

/// L2 行情快照（股票，10档行情）—— 复用 [`MDSnapshot`]
pub type StockL2Snapshot = MDSnapshot;

/// 深度加权买入均价（前 depth 档，depth ≤ 10）
pub fn weighted_bid_price(snap: &MDSnapshot, depth: usize) -> Option<f64> {
    let prices: [Option<f64>; 10] = [
        Some(snap.bid_price1),
        snap.bid_price2,
        snap.bid_price3,
        snap.bid_price4,
        snap.bid_price5,
        snap.bid_price6,
        snap.bid_price7,
        snap.bid_price8,
        snap.bid_price9,
        snap.bid_price10,
    ];
    let volumes: [Option<f64>; 10] = [
        Some(snap.bid_volume1 as f64),
        snap.bid_volume2.map(|x| x as f64),
        snap.bid_volume3.map(|x| x as f64),
        snap.bid_volume4.map(|x| x as f64),
        snap.bid_volume5.map(|x| x as f64),
        snap.bid_volume6.map(|x| x as f64),
        snap.bid_volume7.map(|x| x as f64),
        snap.bid_volume8.map(|x| x as f64),
        snap.bid_volume9.map(|x| x as f64),
        snap.bid_volume10.map(|x| x as f64),
    ];
    let d = depth.min(10);
    let mut total_vol = 0.0f64;
    let mut total_val = 0.0f64;
    for i in 0..d {
        if let (Some(p), Some(v)) = (prices[i], volumes[i]) {
            total_val += p * v;
            total_vol += v;
        }
    }
    if total_vol > 0.0 { Some(total_val / total_vol) } else { None }
}

/// 深度加权卖出均价（前 depth 档，depth ≤ 10）
pub fn weighted_ask_price(snap: &MDSnapshot, depth: usize) -> Option<f64> {
    let prices: [Option<f64>; 10] = [
        Some(snap.ask_price1),
        snap.ask_price2,
        snap.ask_price3,
        snap.ask_price4,
        snap.ask_price5,
        snap.ask_price6,
        snap.ask_price7,
        snap.ask_price8,
        snap.ask_price9,
        snap.ask_price10,
    ];
    let volumes: [Option<f64>; 10] = [
        Some(snap.ask_volume1 as f64),
        snap.ask_volume2.map(|x| x as f64),
        snap.ask_volume3.map(|x| x as f64),
        snap.ask_volume4.map(|x| x as f64),
        snap.ask_volume5.map(|x| x as f64),
        snap.ask_volume6.map(|x| x as f64),
        snap.ask_volume7.map(|x| x as f64),
        snap.ask_volume8.map(|x| x as f64),
        snap.ask_volume9.map(|x| x as f64),
        snap.ask_volume10.map(|x| x as f64),
    ];
    let d = depth.min(10);
    let mut total_vol = 0.0f64;
    let mut total_val = 0.0f64;
    for i in 0..d {
        if let (Some(p), Some(v)) = (prices[i], volumes[i]) {
            total_val += p * v;
            total_vol += v;
        }
    }
    if total_vol > 0.0 { Some(total_val / total_vol) } else { None }
}

/// 买卖方向压力比：(bid_vol - ask_vol) / (bid_vol + ask_vol)，前 depth 档
pub fn order_imbalance(snap: &MDSnapshot, depth: usize) -> Option<f64> {
    let bid_vols: [Option<f64>; 10] = [
        Some(snap.bid_volume1 as f64),
        snap.bid_volume2.map(|x| x as f64),
        snap.bid_volume3.map(|x| x as f64),
        snap.bid_volume4.map(|x| x as f64),
        snap.bid_volume5.map(|x| x as f64),
        snap.bid_volume6.map(|x| x as f64),
        snap.bid_volume7.map(|x| x as f64),
        snap.bid_volume8.map(|x| x as f64),
        snap.bid_volume9.map(|x| x as f64),
        snap.bid_volume10.map(|x| x as f64),
    ];
    let ask_vols: [Option<f64>; 10] = [
        Some(snap.ask_volume1 as f64),
        snap.ask_volume2.map(|x| x as f64),
        snap.ask_volume3.map(|x| x as f64),
        snap.ask_volume4.map(|x| x as f64),
        snap.ask_volume5.map(|x| x as f64),
        snap.ask_volume6.map(|x| x as f64),
        snap.ask_volume7.map(|x| x as f64),
        snap.ask_volume8.map(|x| x as f64),
        snap.ask_volume9.map(|x| x as f64),
        snap.ask_volume10.map(|x| x as f64),
    ];
    let d = depth.min(10);
    let bid_sum: f64 = bid_vols[..d].iter().filter_map(|x| *x).sum();
    let ask_sum: f64 = ask_vols[..d].iter().filter_map(|x| *x).sum();
    let total = bid_sum + ask_sum;
    if total > 0.0 { Some((bid_sum - ask_sum) / total) } else { None }
}
