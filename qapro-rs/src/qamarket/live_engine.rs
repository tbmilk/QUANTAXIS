use crate::qadatastruct::mdsnapshot::MDSnapshot;
use crate::qamarket::live_types::{
    LiveContext, MarketDataEnvelope, MarketDataPullSource, OmsService, SignalGenerator,
};
use crate::qarisk::context::{MarketState, RiskContext};
use crate::qarisk::execution::{BrokerAdapter, OrderAck, TradeReport};
use crate::qarisk::market::MarketType;
use crate::qarisk::service::{RiskDecision, RiskService};

#[derive(Debug, Clone)]
pub struct EngineOrderEvent {
    pub strategy_name: String,
    pub order_id: String,
    pub decision: RiskDecision,
    pub ack: Option<OrderAck>,
}

#[derive(Debug, Clone, Default)]
pub struct EngineRunStats {
    pub market_events: usize,
    pub emitted_orders: usize,
    pub approved_orders: usize,
    pub rejected_orders: usize,
}

/// 最小可运行实盘引擎。
///
/// 当前职责:
/// - 接收标准化行情快照
/// - 调用策略生成信号
/// - 构造 RiskContext 并执行风控评估
/// - 经 BrokerAdapter 提交订单
/// - 将 OrderAck / TradeReport 回写到 OMS
pub struct LiveEngine<'a> {
    market_type: MarketType,
    market_state: MarketState,
    risk_service: &'a RiskService,
    broker: &'a dyn BrokerAdapter,
    oms: &'a mut dyn OmsService,
    strategies: Vec<Box<dyn SignalGenerator>>,
}

impl<'a> LiveEngine<'a> {
    pub fn new(
        market_type: MarketType,
        risk_service: &'a RiskService,
        broker: &'a dyn BrokerAdapter,
        oms: &'a mut dyn OmsService,
    ) -> Self {
        Self {
            market_type,
            market_state: MarketState {
                market_type,
                is_trading: true,
                ..MarketState::default()
            },
            risk_service,
            broker,
            oms,
            strategies: Vec::new(),
        }
    }

    pub fn register_strategy(&mut self, strategy: impl SignalGenerator + 'static) {
        self.strategies.push(Box::new(strategy));
    }

    pub fn strategy_count(&self) -> usize {
        self.strategies.len()
    }

    pub fn process_market_event(
        &mut self,
        envelope: &MarketDataEnvelope,
    ) -> Result<Vec<EngineOrderEvent>, String> {
        self.update_market_state(&envelope.snapshot);

        let mut events = Vec::new();
        for strategy in self.strategies.iter_mut() {
            let strategy_name = strategy.name().to_string();
            let signals = strategy.on_snapshot(&envelope.snapshot)?;
            for signal in signals {
                let portfolio = self.oms.snapshot().portfolio.unwrap_or_default();
                let mut order = signal.order.clone();
                order.market_type = self.market_type;

                let ctx = RiskContext::new(order.clone(), portfolio, self.market_state.clone());
                let decision = self.risk_service.evaluate(&ctx);
                let mut live_ctx = LiveContext {
                    risk_service: self.risk_service,
                    broker: self.broker,
                    oms: self.oms,
                };

                if decision.approved {
                    let ack = live_ctx.evaluate_and_submit(&order, &decision)?;
                    strategy.on_order_ack(&ack)?;
                    events.push(EngineOrderEvent {
                        strategy_name: strategy_name.clone(),
                        order_id: order.order_id.clone(),
                        decision,
                        ack: Some(ack),
                    });
                } else {
                    events.push(EngineOrderEvent {
                        strategy_name: strategy_name.clone(),
                        order_id: order.order_id.clone(),
                        decision,
                        ack: None,
                    });
                }
            }
        }

        Ok(events)
    }

    pub fn apply_trade_report(&mut self, trade: &TradeReport) -> Result<(), String> {
        self.oms.apply_trade_report(trade)?;
        for strategy in self.strategies.iter_mut() {
            strategy.on_trade_report(trade)?;
        }
        Ok(())
    }

    pub fn run_pull_source_once(
        &mut self,
        source: &mut dyn MarketDataPullSource,
    ) -> Result<Option<Vec<EngineOrderEvent>>, String> {
        match source.next_event()? {
            Some(event) => self.process_market_event(&event).map(Some),
            None => Ok(None),
        }
    }

    pub fn run_pull_source_until_exhausted(
        &mut self,
        source: &mut dyn MarketDataPullSource,
    ) -> Result<EngineRunStats, String> {
        let mut stats = EngineRunStats::default();
        while let Some(events) = self.run_pull_source_once(source)? {
            stats.market_events += 1;
            stats.emitted_orders += events.len();
            for event in events {
                if event.decision.approved {
                    stats.approved_orders += 1;
                } else {
                    stats.rejected_orders += 1;
                }
            }
        }
        Ok(stats)
    }

    pub fn market_state(&self) -> &MarketState {
        &self.market_state
    }

    pub fn oms_snapshot(&self) -> crate::qamarket::live_types::OmsSnapshot {
        self.oms.snapshot()
    }

    fn update_market_state(&mut self, snapshot: &MDSnapshot) {
        self.market_state
            .prices
            .insert(snapshot.instrument_id.clone(), snapshot.last_price);
        self.market_state
            .volumes
            .insert(snapshot.instrument_id.clone(), snapshot.volume);
        self.market_state
            .prev_close
            .insert(snapshot.instrument_id.clone(), snapshot.pre_close);
        self.market_state.is_trading = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::qadatastruct::mdsnapshot::OptionalF64;
    use crate::qamarket::live_types::{MarketDataEnvelope, OmsService, Signal, SourceHealth};
    use crate::qamarket::live_types::MarketDataPullSource;
    use crate::qamarket::qaoms::MemoryOmsService;
    use crate::qarisk::context::{Direction, Offset, OrderSnapshot, PortfolioSnapshot};
    use crate::qarisk::execution::{MockBroker, OrderStatus};
    use crate::qamarket::qamdgateway::MarketDataSource as GatewayMarketDataSource;

    struct TestSignalGenerator {
        name: String,
        account_id: String,
        emitted: bool,
    }

    impl TestSignalGenerator {
        fn new(name: &str, account_id: &str) -> Self {
            Self {
                name: name.to_string(),
                account_id: account_id.to_string(),
                emitted: false,
            }
        }
    }

    impl SignalGenerator for TestSignalGenerator {
        fn name(&self) -> &str {
            &self.name
        }

        fn on_snapshot(&mut self, snapshot: &MDSnapshot) -> Result<Vec<Signal>, String> {
            if self.emitted {
                return Ok(Vec::new());
            }
            self.emitted = true;
            Ok(vec![Signal {
                instrument_id: snapshot.instrument_id.clone(),
                source: self.name.clone(),
                strength: 1.0,
                order: OrderSnapshot {
                    order_id: "live-order-1".to_string(),
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

    struct TestPullSource {
        events: Vec<MarketDataEnvelope>,
        cursor: usize,
    }

    impl TestPullSource {
        fn new(events: Vec<MarketDataEnvelope>) -> Self {
            Self { events, cursor: 0 }
        }
    }

    impl crate::qamarket::live_types::MarketDataSource for TestPullSource {
        fn name(&self) -> &str {
            "TestPullSource"
        }

        fn source_type(&self) -> GatewayMarketDataSource {
            GatewayMarketDataSource::Custom
        }

        fn health_check(&self) -> crate::qamarket::live_types::SourceHealth {
            SourceHealth::Healthy
        }

        fn subscribe(&mut self, _instruments: &[String]) -> Result<(), String> {
            Ok(())
        }

        fn unsubscribe(&mut self, _instruments: &[String]) -> Result<(), String> {
            Ok(())
        }
    }

    impl MarketDataPullSource for TestPullSource {
        fn next_event(&mut self) -> Result<Option<MarketDataEnvelope>, String> {
            if self.cursor >= self.events.len() {
                Ok(None)
            } else {
                let event = self.events[self.cursor].clone();
                self.cursor += 1;
                Ok(Some(event))
            }
        }
    }

    fn make_snapshot() -> MDSnapshot {
        MDSnapshot {
            instrument_id: "SHFE.ag2604".to_string(),
            amount: 1000.0,
            ask_price1: 101.0,
            ask_price2: None,
            ask_price3: None,
            ask_price4: None,
            ask_price5: None,
            ask_price6: None,
            ask_price7: None,
            ask_price8: None,
            ask_price9: None,
            ask_price10: None,
            ask_volume1: 10,
            ask_volume2: None,
            ask_volume3: None,
            ask_volume4: None,
            ask_volume5: None,
            ask_volume6: None,
            ask_volume7: None,
            ask_volume8: None,
            ask_volume9: None,
            ask_volume10: None,
            bid_price1: 100.0,
            bid_price2: None,
            bid_price3: None,
            bid_price4: None,
            bid_price5: None,
            bid_price6: None,
            bid_price7: None,
            bid_price8: None,
            bid_price9: None,
            bid_price10: None,
            bid_volume1: 12,
            bid_volume2: None,
            bid_volume3: None,
            bid_volume4: None,
            bid_volume5: None,
            bid_volume6: None,
            bid_volume7: None,
            bid_volume8: None,
            bid_volume9: None,
            bid_volume10: None,
            close: OptionalF64::Null,
            datetime: chrono::Utc::now(),
            highest: 105.0,
            last_price: 100.5,
            lower_limit: 90.0,
            lowest: 95.0,
            open: 98.0,
            open_interest: OptionalF64::Value(10.0),
            pre_close: 99.0,
            pre_open_interest: OptionalF64::Value(9.0),
            pre_settlement: OptionalF64::Value(99.5),
            settlement: OptionalF64::Null,
            upper_limit: 110.0,
            volume: 100,
            average: 100.0,
            iopv: OptionalF64::Null,
        }
    }

    #[test]
    fn test_live_engine_process_market_event() {
        let risk_service = RiskService::new(MarketType::CNFutures, 100_000.0);
        let broker = MockBroker::new("mock", vec![MarketType::CNFutures]);
        let mut oms = MemoryOmsService::new("acc-1").with_portfolio(PortfolioSnapshot {
            account_id: "acc-1".to_string(),
            cash: 100_000.0,
            total_value: 100_000.0,
            ..PortfolioSnapshot::default()
        });
        let mut engine = LiveEngine::new(MarketType::CNFutures, &risk_service, &broker, &mut oms);
        engine.register_strategy(TestSignalGenerator::new("test_strategy", "acc-1"));

        let events = engine
            .process_market_event(&MarketDataEnvelope {
                source: GatewayMarketDataSource::CTP,
                snapshot: make_snapshot(),
                replay: false,
            })
            .unwrap();

        assert_eq!(events.len(), 1);
        assert!(events[0].decision.approved);
        assert_eq!(events[0].ack.as_ref().unwrap().status, OrderStatus::Pending);
        assert_eq!(engine.market_state().prices["SHFE.ag2604"], 100.5);
        assert_eq!(engine.oms.snapshot().orders.len(), 1);
    }

    #[test]
    fn test_live_engine_run_pull_source_until_exhausted() {
        let risk_service = RiskService::new(MarketType::CNFutures, 100_000.0);
        let broker = MockBroker::new("mock", vec![MarketType::CNFutures]);
        let mut oms = MemoryOmsService::new("acc-1").with_portfolio(PortfolioSnapshot {
            account_id: "acc-1".to_string(),
            cash: 100_000.0,
            total_value: 100_000.0,
            ..PortfolioSnapshot::default()
        });
        let mut engine = LiveEngine::new(MarketType::CNFutures, &risk_service, &broker, &mut oms);
        engine.register_strategy(TestSignalGenerator::new("test_strategy", "acc-1"));

        let mut source = TestPullSource::new(vec![MarketDataEnvelope {
            source: GatewayMarketDataSource::Custom,
            snapshot: make_snapshot(),
            replay: true,
        }]);
        let stats = engine.run_pull_source_until_exhausted(&mut source).unwrap();
        assert_eq!(stats.market_events, 1);
        assert_eq!(stats.approved_orders, 1);
        assert_eq!(stats.rejected_orders, 0);
    }
}
