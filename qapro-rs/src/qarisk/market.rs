//! 市场配置（Phase 2）
//!
//! 定义 CN / CNFutures / HK / US / Crypto 各市场的交易规则、
//! 杠杆上限、保证金要求、涨跌停幅度等。

/// 市场类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MarketType {
    /// 中国 A 股（T+1，10% 涨跌停）
    #[default]
    CN,
    /// 中国商品 / 金融期货
    CNFutures,
    /// 香港股票
    HK,
    /// 美国股票
    US,
    /// 加密货币（24×7，无涨跌停）
    Crypto,
}

impl std::fmt::Display for MarketType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            MarketType::CN => "CN",
            MarketType::CNFutures => "CNFutures",
            MarketType::HK => "HK",
            MarketType::US => "US",
            MarketType::Crypto => "Crypto",
        };
        write!(f, "{}", s)
    }
}

/// 交易时段（HH:MM 字符串，本地时区）
#[derive(Debug, Clone)]
pub struct TradingSession {
    pub open: &'static str,
    pub close: &'static str,
    pub label: &'static str,
}

/// 市场配置文件
#[derive(Debug, Clone)]
pub struct MarketProfile {
    pub market_type: MarketType,
    /// 最大杠杆倍数（1.0 = 不允许杠杆，期货通常 5~20）
    pub max_leverage: f64,
    /// 保证金比例（0 ~ 1）
    pub margin_rate: f64,
    /// 单日涨跌停幅度（None = 无限制）
    pub price_limit_pct: Option<f64>,
    /// 最小变动价位
    pub tick_size: f64,
    /// 最小交易单位（手）
    pub lot_size: i64,
    /// 单一持仓集中度上限（单仓市值 / 总资产）
    pub max_concentration: f64,
    /// 单笔订单最大金额（0.0 = 不限制）
    pub max_order_value: f64,
    /// 是否允许卖空
    pub allow_short: bool,
    /// 是否 T+0
    pub t_plus_zero: bool,
    /// 交易时段
    pub sessions: &'static [TradingSession],
}

// ─── 各市场默认配置 ───────────────────────────────────────────────────────────

static CN_SESSIONS: &[TradingSession] = &[
    TradingSession { open: "09:30", close: "11:30", label: "上午盘" },
    TradingSession { open: "13:00", close: "15:00", label: "下午盘" },
];

static CN_FUTURES_SESSIONS: &[TradingSession] = &[
    TradingSession { open: "09:00", close: "11:30", label: "上午盘" },
    TradingSession { open: "13:30", close: "15:00", label: "下午盘" },
    TradingSession { open: "21:00", close: "23:30", label: "夜盘" },
];

static HK_SESSIONS: &[TradingSession] = &[
    TradingSession { open: "09:30", close: "12:00", label: "上午盘" },
    TradingSession { open: "13:00", close: "16:00", label: "下午盘" },
];

static US_SESSIONS: &[TradingSession] = &[
    TradingSession { open: "09:30", close: "16:00", label: "正常盘" },
];

static CRYPTO_SESSIONS: &[TradingSession] = &[];

impl MarketProfile {
    pub fn cn() -> Self {
        Self {
            market_type: MarketType::CN,
            max_leverage: 1.0,
            margin_rate: 1.0,
            price_limit_pct: Some(0.10),
            tick_size: 0.01,
            lot_size: 100,
            max_concentration: 0.30,
            max_order_value: 0.0,
            allow_short: false,
            t_plus_zero: false,
            sessions: CN_SESSIONS,
        }
    }

    pub fn cn_futures() -> Self {
        Self {
            market_type: MarketType::CNFutures,
            max_leverage: 20.0,
            margin_rate: 0.05,
            price_limit_pct: Some(0.05),
            tick_size: 1.0,
            lot_size: 1,
            max_concentration: 0.50,
            max_order_value: 0.0,
            allow_short: true,
            t_plus_zero: true,
            sessions: CN_FUTURES_SESSIONS,
        }
    }

    pub fn hk() -> Self {
        Self {
            market_type: MarketType::HK,
            max_leverage: 2.0,
            margin_rate: 0.50,
            price_limit_pct: None,
            tick_size: 0.01,
            lot_size: 1,
            max_concentration: 0.25,
            max_order_value: 0.0,
            allow_short: true,
            t_plus_zero: false,
            sessions: HK_SESSIONS,
        }
    }

    pub fn us() -> Self {
        Self {
            market_type: MarketType::US,
            max_leverage: 2.0,
            margin_rate: 0.50,
            price_limit_pct: None,
            tick_size: 0.01,
            lot_size: 1,
            max_concentration: 0.20,
            max_order_value: 0.0,
            allow_short: true,
            t_plus_zero: true,
            sessions: US_SESSIONS,
        }
    }

    pub fn crypto() -> Self {
        Self {
            market_type: MarketType::Crypto,
            max_leverage: 10.0,
            margin_rate: 0.10,
            price_limit_pct: None,
            tick_size: 0.0001,
            lot_size: 1,
            max_concentration: 0.50,
            max_order_value: 0.0,
            allow_short: true,
            t_plus_zero: true,
            sessions: CRYPTO_SESSIONS,
        }
    }

    /// 按市场类型构建默认配置
    pub fn for_market(mt: MarketType) -> Self {
        match mt {
            MarketType::CN => Self::cn(),
            MarketType::CNFutures => Self::cn_futures(),
            MarketType::HK => Self::hk(),
            MarketType::US => Self::us(),
            MarketType::Crypto => Self::crypto(),
        }
    }

    /// 给定参考价，判断报价是否合法（是否在涨跌停范围内）
    pub fn is_price_valid(&self, price: f64, ref_price: f64) -> bool {
        if price <= 0.0 || ref_price <= 0.0 {
            return false;
        }
        match self.price_limit_pct {
            None => true,
            Some(limit) => {
                let upper = ref_price * (1.0 + limit);
                let lower = ref_price * (1.0 - limit);
                price >= lower - f64::EPSILON && price <= upper + f64::EPSILON
            }
        }
    }

    /// 合约价值（价格 × 数量 × 合约乘数，合约乘数默认 1）
    pub fn contract_value(&self, price: f64, volume: i64) -> f64 {
        price * (volume * self.lot_size) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cn_price_limit() {
        let p = MarketProfile::cn();
        assert!(p.is_price_valid(10.9, 10.0));
        assert!(!p.is_price_valid(11.1, 10.0));
        assert!(!p.is_price_valid(8.9, 10.0));
    }

    #[test]
    fn test_crypto_no_limit() {
        let p = MarketProfile::crypto();
        assert!(p.is_price_valid(0.0001, 1.0));
        assert!(p.is_price_valid(1_000_000.0, 1.0));
    }

    #[test]
    fn test_for_market() {
        assert_eq!(MarketProfile::for_market(MarketType::HK).market_type, MarketType::HK);
    }
}
