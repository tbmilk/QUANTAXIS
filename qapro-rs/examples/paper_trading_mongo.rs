use std::env;

use qapro_rs::qaconnector::mongo::replay::{load_replay_source, MongoReplayCollectionConfig};
use qapro_rs::qamarket::live_engine::LiveEngine;
use qapro_rs::qamarket::live_types::{MarketDataSource, Signal, SignalGenerator};
use qapro_rs::qamarket::qaoms::MemoryOmsService;
use qapro_rs::qamarket::qamdgateway::MarketDataSource as GatewayMarketDataSource;
use qapro_rs::qarisk::context::{Direction, Offset, OrderSnapshot, PortfolioSnapshot};
use qapro_rs::qarisk::execution::MockBroker;
use qapro_rs::qarisk::market::MarketType;
use qapro_rs::qarisk::service::RiskService;

struct ReplayBuyOnceStrategy {
    account_id: String,
    emitted: bool,
}

impl ReplayBuyOnceStrategy {
    fn new(account_id: &str) -> Self {
        Self {
            account_id: account_id.to_string(),
            emitted: false,
        }
    }
}

impl SignalGenerator for ReplayBuyOnceStrategy {
    fn name(&self) -> &str {
        "ReplayBuyOnceStrategy"
    }

    fn on_snapshot(&mut self, snapshot: &qapro_rs::qadatastruct::mdsnapshot::MDSnapshot) -> Result<Vec<Signal>, String> {
        if self.emitted {
            return Ok(Vec::new());
        }
        self.emitted = true;
        Ok(vec![Signal {
            instrument_id: snapshot.instrument_id.clone(),
            source: self.name().to_string(),
            strength: 1.0,
            order: OrderSnapshot {
                order_id: format!("paper-{}", snapshot.datetime.timestamp_millis()),
                instrument_id: snapshot.instrument_id.clone(),
                direction: Direction::Buy,
                offset: Offset::Open,
                price: snapshot.last_price,
                volume: 1,
                market_type: MarketType::CNFutures,
                account_id: self.account_id.clone(),
            },
        }])
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn main() -> Result<(), String> {
    let uri = env_or("QA_MONGO_URI", "mongodb://localhost:27017");
    let database = env_or("QA_MONGO_DATABASE", "quantaxis");
    let collection = env_or("QA_REPLAY_COLLECTION", "future_min");
    let codes = env_or("QA_REPLAY_CODES", "AGL8")
        .split(',')
        .filter(|item| !item.trim().is_empty())
        .map(|item| item.trim().to_string())
        .collect::<Vec<_>>();
    let start = env_or("QA_REPLAY_START", "2024-01-01");
    let end = env_or("QA_REPLAY_END", "2024-01-31");
    let account_id = env_or("QA_PAPER_ACCOUNT_ID", "paper-acc-1");

    let replay_cfg = MongoReplayCollectionConfig::new(
        uri,
        database,
        collection,
        GatewayMarketDataSource::Custom,
    );
    let mut replay = load_replay_source(&replay_cfg, "MongoPaperReplay", &codes, &start, &end)?;
    replay.subscribe(&codes)?;

    let risk_service = RiskService::new(MarketType::CNFutures, 100_000.0);
    let broker = MockBroker::new("paper-broker", vec![MarketType::CNFutures]);
    let mut oms = MemoryOmsService::new(&account_id).with_portfolio(PortfolioSnapshot {
        account_id: account_id.clone(),
        cash: 100_000.0,
        total_value: 100_000.0,
        ..PortfolioSnapshot::default()
    });
    let mut engine = LiveEngine::new(MarketType::CNFutures, &risk_service, &broker, &mut oms);
    engine.register_strategy(ReplayBuyOnceStrategy::new(&account_id));

    let stats = engine.run_pull_source_until_exhausted(&mut replay)?;
    println!(
        "paper replay done: market_events={}, emitted_orders={}, approved_orders={}, rejected_orders={}",
        stats.market_events, stats.emitted_orders, stats.approved_orders, stats.rejected_orders
    );

    let snapshot = engine.market_state();
    println!("last market instruments: {}", snapshot.prices.len());
    let oms_snapshot = engine.oms_snapshot();
    println!(
        "oms summary: orders={}, trades={}, positions={}",
        oms_snapshot.orders.len(),
        oms_snapshot.trades.len(),
        oms_snapshot
            .portfolio
            .as_ref()
            .map(|portfolio| portfolio.positions.len())
            .unwrap_or(0)
    );
    Ok(())
}
