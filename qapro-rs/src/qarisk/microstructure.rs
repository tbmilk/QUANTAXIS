//! 微结构风控引擎（Phase 4）
//!
//! 提供：
//! - 订单簿模型（市场冲击估计）
//! - 滑点估计（线性/平方根冲击模型）
//! - 流动性检查（深度、换手率、冲击成本）
//! - 最优执行拆单建议（TWAP / VWAP 分拆）

use crate::qarisk::context::{OrderBook, OrderSnapshot};

// ─── 流动性评估结果 ───────────────────────────────────────────────────────────

/// 流动性检查结果
#[derive(Debug, Clone)]
pub struct LiquidityCheck {
    pub instrument_id: String,
    /// 当前可成交量（不超过可用深度）
    pub available_volume: i64,
    /// 估计滑点（点数）
    pub estimated_slippage: f64,
    /// 估计冲击成本（订单金额的比例）
    pub market_impact_bps: f64,
    /// 流动性是否充足
    pub is_liquid: bool,
    /// 建议拆单数量（0 = 无需拆单）
    pub suggested_split: i64,
    pub message: String,
}

impl LiquidityCheck {
    pub fn insufficient(instrument_id: &str, reason: &str) -> Self {
        Self {
            instrument_id: instrument_id.to_string(),
            available_volume: 0,
            estimated_slippage: f64::INFINITY,
            market_impact_bps: f64::INFINITY,
            is_liquid: false,
            suggested_split: 0,
            message: reason.to_string(),
        }
    }
}

// ─── 微结构引擎 ───────────────────────────────────────────────────────────────

/// 微结构风控引擎
pub struct MicrostructureEngine {
    /// 流动性阈值：订单量不超过当前 N 档深度的比例
    pub max_participation_rate: f64,
    /// 最大可接受市场冲击（bps）
    pub max_impact_bps: f64,
    /// 拆单阈值：超过可用量的此比例时建议拆单
    pub split_threshold: f64,
}

impl Default for MicrostructureEngine {
    fn default() -> Self {
        Self {
            max_participation_rate: 0.20, // 订单量不超过深度 5 档的 20%
            max_impact_bps: 50.0,         // 最大冲击 50 bps
            split_threshold: 0.10,        // 超过 10% 则建议拆单
        }
    }
}

impl MicrostructureEngine {
    pub fn new(max_participation_rate: f64, max_impact_bps: f64) -> Self {
        Self { max_participation_rate, max_impact_bps, ..Default::default() }
    }

    /// 估计市价单的平均成交价（模拟吃单）
    ///
    /// 按订单簿逐档消耗，返回 (平均成交价, 实际可成交量)
    pub fn simulate_fill(
        &self,
        order: &OrderSnapshot,
        book: &OrderBook,
    ) -> (f64, i64) {
        let levels = if order.is_buy() { &book.asks } else { &book.bids };
        if levels.is_empty() {
            return (order.price, 0);
        }

        let mut remaining = order.volume;
        let mut total_cost = 0.0;
        let mut total_filled = 0_i64;

        for &(price, avail) in levels {
            if remaining <= 0 { break; }
            let fill = remaining.min(avail);
            total_cost += price * fill as f64;
            total_filled += fill;
            remaining -= fill;
        }

        let avg_price = if total_filled > 0 { total_cost / total_filled as f64 } else { 0.0 };
        (avg_price, total_filled)
    }

    /// 估计滑点（相对于买一/卖一价的偏差）
    pub fn estimate_slippage(
        &self,
        order: &OrderSnapshot,
        book: &OrderBook,
    ) -> f64 {
        let (avg_fill, filled) = self.simulate_fill(order, book);
        if filled == 0 { return f64::INFINITY; }

        let reference = if order.is_buy() {
            book.best_ask().unwrap_or(order.price)
        } else {
            book.best_bid().unwrap_or(order.price)
        };

        if reference == 0.0 { return 0.0; }
        (avg_fill - reference).abs() / reference
    }

    /// 平方根市场冲击模型（Almgren et al.）
    ///
    /// 冲击（bps）= η × σ × √(X / ADV)
    /// 其中 η ≈ 0.1，σ = 日波动率，X = 订单量，ADV = 日均成交量
    pub fn sqrt_impact_bps(
        &self,
        order_volume: i64,
        adv: i64,  // 日均成交量
        daily_vol: f64,  // 日波动率（如 0.02）
    ) -> f64 {
        if adv <= 0 { return f64::INFINITY; }
        let participation = order_volume as f64 / adv as f64;
        0.1 * daily_vol * participation.sqrt() * 10_000.0 // 转为 bps
    }

    /// 全面流动性检查
    pub fn check_liquidity(
        &self,
        order: &OrderSnapshot,
        book: &OrderBook,
        adv: i64,
        daily_vol: f64,
    ) -> LiquidityCheck {
        // 订单簿是否存在
        if book.bids.is_empty() && book.asks.is_empty() {
            return LiquidityCheck::insufficient(&order.instrument_id, "订单簿为空");
        }

        // 可用深度（前 5 档）
        let available = if order.is_buy() {
            book.ask_depth(5)
        } else {
            book.bid_depth(5)
        };

        // 市场冲击估计
        let impact_bps = self.sqrt_impact_bps(order.volume, adv, daily_vol);
        let slippage = self.estimate_slippage(order, book);

        // 流动性判断
        let participation = if available > 0 { order.volume as f64 / available as f64 } else { f64::INFINITY };
        let is_liquid = participation <= self.max_participation_rate
            && impact_bps <= self.max_impact_bps;

        // 可实际成交量
        let available_volume = order.volume.min(available);

        // 拆单建议
        let suggested_split = if participation > self.split_threshold && adv > 0 {
            // 建议按 5% ADV 拆单
            (adv as f64 * 0.05) as i64
        } else {
            0
        };

        let message = if is_liquid {
            format!("流动性正常，冲击 {:.1} bps", impact_bps)
        } else {
            format!(
                "流动性不足：参与率 {:.1}%，冲击 {:.1} bps",
                participation * 100.0,
                impact_bps
            )
        };

        LiquidityCheck {
            instrument_id: order.instrument_id.clone(),
            available_volume,
            estimated_slippage: slippage,
            market_impact_bps: impact_bps,
            is_liquid,
            suggested_split,
            message,
        }
    }

    /// TWAP 拆单建议
    ///
    /// 将大单拆成 `n_slices` 份，每份相隔 `interval_minutes` 分钟
    pub fn twap_slices(
        &self,
        total_volume: i64,
        n_slices: usize,
    ) -> Vec<i64> {
        if n_slices == 0 { return vec![]; }
        let base = total_volume / n_slices as i64;
        let remainder = total_volume - base * n_slices as i64;
        let mut slices = vec![base; n_slices];
        slices[0] += remainder; // 余量放在第一份
        slices
    }

    /// VWAP 拆单建议（按历史成交量分布加权）
    ///
    /// `volume_profile`：各时间段历史成交量占比（sum = 1）
    pub fn vwap_slices(
        &self,
        total_volume: i64,
        volume_profile: &[f64],
    ) -> Vec<i64> {
        let profile_sum: f64 = volume_profile.iter().sum();
        if profile_sum < 1e-10 {
            return self.twap_slices(total_volume, volume_profile.len());
        }
        let mut slices: Vec<i64> = volume_profile.iter()
            .map(|p| (total_volume as f64 * p / profile_sum).round() as i64)
            .collect();
        // 调整总量
        let diff = total_volume - slices.iter().sum::<i64>();
        if !slices.is_empty() { slices[0] += diff; }
        slices
    }
}

// ─── 价格冲击模型（线性） ─────────────────────────────────────────────────────

/// 线性价格冲击模型
///
/// 冲击价格 = 参考价 × (1 ± λ × (X / ADV))
/// 买单：+，卖单：-
pub fn linear_impact_price(
    ref_price: f64,
    order_volume: i64,
    adv: i64,
    lambda: f64,
    is_buy: bool,
) -> f64 {
    if adv <= 0 { return ref_price; }
    let pct = lambda * order_volume as f64 / adv as f64;
    if is_buy { ref_price * (1.0 + pct) } else { ref_price * (1.0 - pct) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qarisk::context::{Direction, Offset, OrderBook};
    use crate::qarisk::market::MarketType;

    fn make_order(vol: i64) -> OrderSnapshot {
        OrderSnapshot {
            order_id: "o1".into(),
            instrument_id: "rb2501".into(),
            direction: Direction::Buy,
            offset: Offset::Open,
            price: 3500.0,
            volume: vol,
            market_type: MarketType::CNFutures,
            account_id: "acc1".into(),
        }
    }

    fn make_book() -> OrderBook {
        OrderBook {
            instrument_id: "rb2501".into(),
            bids: vec![(3498.0, 50), (3496.0, 100), (3494.0, 200)],
            asks: vec![(3500.0, 30), (3502.0, 80), (3504.0, 150), (3506.0, 200), (3508.0, 300)],
            timestamp_ms: 0,
        }
    }

    #[test]
    fn test_simulate_fill_partial() {
        let engine = MicrostructureEngine::default();
        let order = make_order(200); // 需要 200 手，但 ask 1 档只有 30
        let book = make_book();
        let (avg_price, filled) = engine.simulate_fill(&order, &book);
        assert!(filled <= 200);
        assert!(avg_price >= 3500.0); // 买单成交价应 >= 卖一价
        println!("avg_price={:.2}, filled={}", avg_price, filled);
    }

    #[test]
    fn test_check_liquidity_liquid() {
        let engine = MicrostructureEngine::new(0.50, 100.0);
        let order = make_order(10); // 小单
        let book = make_book();
        let check = engine.check_liquidity(&order, &book, 100_000, 0.02);
        assert!(check.is_liquid);
    }

    #[test]
    fn test_check_liquidity_illiquid() {
        let engine = MicrostructureEngine::new(0.05, 10.0); // 严格限制
        let order = make_order(500); // 大单
        let book = make_book();
        let check = engine.check_liquidity(&order, &book, 1_000, 0.02);
        // 参与率高，流动性不足
        assert!(!check.is_liquid || check.market_impact_bps > 10.0);
    }

    #[test]
    fn test_twap_slices_sum() {
        let engine = MicrostructureEngine::default();
        let slices = engine.twap_slices(1001, 5);
        assert_eq!(slices.iter().sum::<i64>(), 1001);
        assert_eq!(slices.len(), 5);
    }

    #[test]
    fn test_vwap_slices_sum() {
        let engine = MicrostructureEngine::default();
        let profile = vec![0.15, 0.20, 0.10, 0.25, 0.30];
        let slices = engine.vwap_slices(1000, &profile);
        assert_eq!(slices.iter().sum::<i64>(), 1000);
    }

    #[test]
    fn test_linear_impact() {
        let p = linear_impact_price(100.0, 10_000, 100_000, 0.5, true);
        // 参与率 10%，冲击 5%
        assert!((p - 105.0).abs() < 0.01);
    }

    #[test]
    fn test_sqrt_impact_bps() {
        let engine = MicrostructureEngine::default();
        let bps = engine.sqrt_impact_bps(10_000, 100_000, 0.02);
        assert!(bps > 0.0 && bps.is_finite());
        println!("sqrt_impact_bps = {:.2}", bps);
    }
}
