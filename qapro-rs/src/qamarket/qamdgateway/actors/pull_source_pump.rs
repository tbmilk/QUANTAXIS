use std::time::Duration;

use actix::prelude::*;
use log::{debug, error, info};

use crate::qamarket::live_types::MarketDataPullSource;
use crate::qamarket::qamdgateway::actors::md_distributor::MarketDataDistributor;
use crate::qamarket::qamdgateway::actors::messages::MarketDataUpdate;

/// 将 `MarketDataPullSource` 接入现有 `MarketDataDistributor` 的桥接 Actor。
///
/// 适用于:
/// - MongoReplaySource
/// - CTPMdSource
/// - 后续 QMT pull/polling 版本
pub struct PullSourcePump {
    source: Box<dyn MarketDataPullSource>,
    distributor: Addr<MarketDataDistributor>,
    poll_interval: Duration,
}

impl PullSourcePump {
    pub fn new(
        source: Box<dyn MarketDataPullSource>,
        distributor: Addr<MarketDataDistributor>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            source,
            distributor,
            poll_interval,
        }
    }

    pub fn with_interval_millis(
        source: Box<dyn MarketDataPullSource>,
        distributor: Addr<MarketDataDistributor>,
        poll_interval_ms: u64,
    ) -> Self {
        Self::new(source, distributor, Duration::from_millis(poll_interval_ms))
    }

    fn drain_once(&mut self) {
        loop {
            match self.source.next_event() {
                Ok(Some(envelope)) => {
                    self.distributor
                        .do_send(MarketDataUpdate(envelope.snapshot, envelope.source));
                }
                Ok(None) => break,
                Err(err) => {
                    error!("pull source `{}` next_event error: {}", self.source.name(), err);
                    break;
                }
            }
        }
    }
}

impl Actor for PullSourcePump {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!(
            "PullSourcePump started: source={}, interval_ms={}",
            self.source.name(),
            self.poll_interval.as_millis()
        );
        ctx.run_interval(self.poll_interval, |act, _ctx| {
            act.drain_once();
        });
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        debug!("PullSourcePump stopped: source={}", self.source.name());
    }
}
