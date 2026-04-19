#![allow(dead_code)]
//! CTP(openctp) 行情/交易接入骨架。
//!
//! 第一阶段主路线:
//! - openctp TTS
//! - ctp2rs(upstream)
//!
//! 这里优先实现行情链路:
//! - 前置连接
//! - 登录回调
//! - 订阅管理
//! - DepthMarketData -> MDSnapshot
//!
//! 行为上参考 QACTPBeeBroker 已验证过的处理方式:
//! 连接前置 -> 回调触发登录 -> 登录成功后批量订阅 -> 将 tick 推入统一主链路。

use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};

use actix::{Actor, Addr};
use crate::qamarket::live_types::{
    MarketDataEnvelope, MarketDataPullSource, MarketDataSource, SourceHealth,
};
use crate::qamarket::qamdgateway::{MarketDataDistributor, PullSourcePump};
use crate::qamarket::qamdgateway::MarketDataSource as GatewayMarketDataSource;

#[cfg(feature = "openctp")]
use ctp2rs::ffi::{AssignFromString, WrapToString};
#[cfg(feature = "openctp")]
use ctp2rs::v1alpha1::{
    CThostFtdcDepthMarketDataField, CThostFtdcReqUserLoginField, CThostFtdcRspInfoField, MdApi,
    MdSpi,
};

#[derive(Debug, Clone)]
pub struct OpenCtpConfig {
    pub md_front: String,
    pub td_front: String,
    pub broker_id: String,
    pub user_id: String,
    pub password: String,
    pub app_id: Option<String>,
    pub auth_code: Option<String>,
    pub flow_path: String,
}

impl OpenCtpConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.md_front.is_empty() || self.td_front.is_empty() {
            return Err("md_front/td_front 不能为空".to_string());
        }
        if self.broker_id.is_empty() || self.user_id.is_empty() {
            return Err("broker_id/user_id 不能为空".to_string());
        }
        Ok(())
    }
}

/// CTP 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtpConnectionState {
    Disconnected,
    Connecting,
    Connected,
    LoggedIn,
    LoginFailed,
}

#[derive(Debug)]
struct CtpMdSharedState {
    connection_state: CtpConnectionState,
    subscriptions: Vec<String>,
    last_error: Option<String>,
}

impl Default for CtpMdSharedState {
    fn default() -> Self {
        Self {
            connection_state: CtpConnectionState::Disconnected,
            subscriptions: Vec::new(),
            last_error: None,
        }
    }
}

#[derive(Debug)]
enum CtpMdEvent {
    Status(CtpConnectionState),
    Error(String),
    Subscribed(String),
    Envelope(MarketDataEnvelope),
}

#[cfg(feature = "openctp")]
struct CtpMdRuntime {
    api: Arc<MdApi>,
    spi_ptr: *mut dyn MdSpi,
}

#[cfg(feature = "openctp")]
unsafe impl Send for CtpMdRuntime {}

#[cfg(feature = "openctp")]
impl Drop for CtpMdRuntime {
    fn drop(&mut self) {
        self.api.release();
        unsafe {
            let _ = Box::from_raw(self.spi_ptr);
        }
    }
}

/// openctp 行情源
pub struct CTPMdSource {
    pub config: OpenCtpConfig,
    pub runtime_config: Option<OpenCtpRuntimeConfig>,
    subscriptions: Vec<String>,
    shared: Arc<Mutex<CtpMdSharedState>>,
    event_rx: Option<Receiver<CtpMdEvent>>,
    #[cfg(feature = "openctp")]
    runtime: Option<CtpMdRuntime>,
}

impl CTPMdSource {
    pub fn new(config: OpenCtpConfig) -> Self {
        Self {
            config,
            runtime_config: None,
            subscriptions: Vec::new(),
            shared: Arc::new(Mutex::new(CtpMdSharedState::default())),
            event_rx: None,
            #[cfg(feature = "openctp")]
            runtime: None,
        }
    }

    pub fn with_runtime_config(mut self, runtime_config: OpenCtpRuntimeConfig) -> Self {
        self.runtime_config = Some(runtime_config);
        self
    }

    pub fn connection_state(&self) -> CtpConnectionState {
        self.shared
            .lock()
            .map(|state| state.connection_state)
            .unwrap_or(CtpConnectionState::Disconnected)
    }

    pub fn last_error(&self) -> Option<String> {
        self.shared.lock().ok().and_then(|state| state.last_error.clone())
    }

    pub fn start(&mut self) -> Result<(), String> {
        self.config.validate()?;
        let runtime = self
            .runtime_config
            .clone()
            .ok_or_else(|| "runtime_config 未配置".to_string())?;
        runtime.validate()?;

        #[cfg(not(feature = "openctp"))]
        {
            let _ = runtime;
            return Err("当前未启用 openctp feature，无法启动 CTPMdSource".to_string());
        }

        #[cfg(feature = "openctp")]
        {
            if !Path::new(&runtime.md_dynlib_path).exists() {
                return Err(format!(
                    "openctp md 动态库不存在: {}",
                    runtime.md_dynlib_path
                ));
            }
            std::fs::create_dir_all(&runtime.flow_dir)
                .map_err(|err| format!("创建 flow_dir 失败: {err}"))?;

            let api = std::panic::catch_unwind(|| {
                MdApi::create_api(&runtime.md_dynlib_path, &runtime.flow_dir, false, false)
            })
            .map_err(|_| "MdApi::create_api panic，请检查动态库版本和部署路径".to_string())?;
            let api = Arc::new(api);
            let (tx, rx) = sync_channel(4096);

            {
                let mut shared = self
                    .shared
                    .lock()
                    .map_err(|_| "共享状态加锁失败".to_string())?;
                shared.connection_state = CtpConnectionState::Connecting;
                shared.subscriptions = self.subscriptions.clone();
                shared.last_error = None;
            }

            let spi = Box::new(CtpMdSpi {
                api: Arc::clone(&api),
                config: self.config.clone(),
                shared: Arc::clone(&self.shared),
                tx,
            });
            let spi_ptr = Box::into_raw(spi) as *mut dyn MdSpi;

            api.register_front(&self.config.md_front);
            api.register_spi(spi_ptr);
            api.init();

            self.event_rx = Some(rx);
            self.runtime = Some(CtpMdRuntime { api, spi_ptr });
            Ok(())
        }
    }

    fn set_last_error(&self, error: String) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.last_error = Some(error);
        }
    }

    #[cfg(feature = "openctp")]
    fn subscribe_live(&self, instruments: &[String]) -> Result<(), String> {
        if instruments.is_empty() {
            return Ok(());
        }
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| "CTPMdSource 尚未启动".to_string())?;
        let ret = runtime.api.subscribe_market_data(&instruments.to_vec());
        if ret == 0 {
            Ok(())
        } else {
            Err(format!("subscribe_market_data 返回错误码: {ret}"))
        }
    }

    #[cfg(feature = "openctp")]
    fn unsubscribe_live(&self, instruments: &[String]) -> Result<(), String> {
        if instruments.is_empty() {
            return Ok(());
        }
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| "CTPMdSource 尚未启动".to_string())?;
        let ret = runtime.api.unsubscribe_market_data(&instruments.to_vec());
        if ret == 0 {
            Ok(())
        } else {
            Err(format!("unsubscribe_market_data 返回错误码: {ret}"))
        }
    }
}

pub fn start_ctp_md_pump(
    mut source: CTPMdSource,
    distributor: Addr<MarketDataDistributor>,
    poll_interval_ms: u64,
) -> Result<Addr<PullSourcePump>, String> {
    source.start()?;
    Ok(PullSourcePump::with_interval_millis(
        Box::new(source),
        distributor,
        poll_interval_ms,
    )
    .start())
}

impl MarketDataSource for CTPMdSource {
    fn name(&self) -> &str {
        "CTPMdSource"
    }

    fn source_type(&self) -> GatewayMarketDataSource {
        GatewayMarketDataSource::CTP
    }

    fn health_check(&self) -> SourceHealth {
        match self.connection_state() {
            CtpConnectionState::LoggedIn => SourceHealth::Healthy,
            CtpConnectionState::Connecting | CtpConnectionState::Connected => SourceHealth::Degraded,
            CtpConnectionState::Disconnected | CtpConnectionState::LoginFailed => SourceHealth::Down,
        }
    }

    fn subscribe(&mut self, instruments: &[String]) -> Result<(), String> {
        for item in instruments {
            if !self.subscriptions.iter().any(|current| current == item) {
                self.subscriptions.push(item.clone());
            }
        }
        if let Ok(mut shared) = self.shared.lock() {
            shared.subscriptions = self.subscriptions.clone();
        }

        #[cfg(feature = "openctp")]
        if self.connection_state() == CtpConnectionState::LoggedIn {
            self.subscribe_live(instruments)?;
        }

        Ok(())
    }

    fn unsubscribe(&mut self, instruments: &[String]) -> Result<(), String> {
        self.subscriptions
            .retain(|code| !instruments.iter().any(|item| item == code));
        if let Ok(mut shared) = self.shared.lock() {
            shared.subscriptions = self.subscriptions.clone();
        }

        #[cfg(feature = "openctp")]
        if self.runtime.is_some() {
            self.unsubscribe_live(instruments)?;
        }

        Ok(())
    }
}

impl MarketDataPullSource for CTPMdSource {
    fn next_event(&mut self) -> Result<Option<MarketDataEnvelope>, String> {
        let Some(receiver) = self.event_rx.as_ref() else {
            return Ok(None);
        };
        loop {
            match receiver.try_recv() {
                Ok(CtpMdEvent::Envelope(envelope)) => return Ok(Some(envelope)),
                Ok(CtpMdEvent::Status(status)) => {
                    if let Ok(mut shared) = self.shared.lock() {
                        shared.connection_state = status;
                    }
                }
                Ok(CtpMdEvent::Subscribed(_instrument)) => {}
                Ok(CtpMdEvent::Error(error)) => {
                    self.set_last_error(error.clone());
                    return Err(error);
                }
                Err(TryRecvError::Empty) => return Ok(None),
                Err(TryRecvError::Disconnected) => {
                    self.set_last_error("CTP 行情事件通道已断开".to_string());
                    return Err("CTP 行情事件通道已断开".to_string());
                }
            }
        }
    }
}

/// openctp 交易适配器骨架
pub struct CTPTrader {
    pub config: OpenCtpConfig,
}

impl CTPTrader {
    pub fn new(config: OpenCtpConfig) -> Self {
        Self { config }
    }
}

/// openctp/ctp2rs 接入层统一配置
#[derive(Debug, Clone)]
pub struct OpenCtpRuntimeConfig {
    pub md_dynlib_path: String,
    pub td_dynlib_path: String,
    pub flow_dir: String,
    pub broker_id: String,
    pub user_id: String,
    pub password: String,
    pub app_id: Option<String>,
    pub auth_code: Option<String>,
    pub use_tts: bool,
}

impl OpenCtpRuntimeConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.md_dynlib_path.is_empty() || self.td_dynlib_path.is_empty() {
            return Err("md_dynlib_path/td_dynlib_path 不能为空".to_string());
        }
        if self.flow_dir.is_empty() {
            return Err("flow_dir 不能为空".to_string());
        }
        if self.broker_id.is_empty() || self.user_id.is_empty() {
            return Err("broker_id/user_id 不能为空".to_string());
        }
        Ok(())
    }
}

#[cfg(feature = "openctp")]
struct CtpMdSpi {
    api: Arc<MdApi>,
    config: OpenCtpConfig,
    shared: Arc<Mutex<CtpMdSharedState>>,
    tx: SyncSender<CtpMdEvent>,
}

#[cfg(feature = "openctp")]
impl MdSpi for CtpMdSpi {
    fn on_front_connected(&mut self) {
        let _ = self.tx.send(CtpMdEvent::Status(CtpConnectionState::Connected));
        let mut req = CThostFtdcReqUserLoginField::default();
        req.BrokerID.assign_from_str(&self.config.broker_id);
        req.UserID.assign_from_str(&self.config.user_id);
        req.Password.assign_from_str(&self.config.password);
        self.api.req_user_login(&mut req, 1);
    }

    fn on_front_disconnected(&mut self, _n_reason: i32) {
        let _ = self.tx.send(CtpMdEvent::Status(CtpConnectionState::Disconnected));
    }

    fn on_rsp_user_login(
        &mut self,
        _p_rsp_user_login: Option<&ctp2rs::v1alpha1::CThostFtdcRspUserLoginField>,
        p_rsp_info: Option<&CThostFtdcRspInfoField>,
        _n_request_id: i32,
        b_is_last: bool,
    ) {
        if !b_is_last {
            return;
        }
        if let Some(error) = rsp_error_message(p_rsp_info) {
            if let Ok(mut shared) = self.shared.lock() {
                shared.connection_state = CtpConnectionState::LoginFailed;
                shared.last_error = Some(error.clone());
            }
            let _ = self.tx.send(CtpMdEvent::Status(CtpConnectionState::LoginFailed));
            let _ = self.tx.send(CtpMdEvent::Error(error));
            return;
        }

        let subscriptions = self
            .shared
            .lock()
            .map(|shared| shared.subscriptions.clone())
            .unwrap_or_default();
        if let Ok(mut shared) = self.shared.lock() {
            shared.connection_state = CtpConnectionState::LoggedIn;
            shared.last_error = None;
        }
        let _ = self.tx.send(CtpMdEvent::Status(CtpConnectionState::LoggedIn));

        if !subscriptions.is_empty() {
            let ret = self.api.subscribe_market_data(&subscriptions);
            if ret != 0 {
                let _ = self.tx.send(CtpMdEvent::Error(format!(
                    "登录后订阅失败，返回码: {ret}"
                )));
            }
        }
    }

    fn on_rsp_error(
        &mut self,
        p_rsp_info: Option<&CThostFtdcRspInfoField>,
        _n_request_id: i32,
        _b_is_last: bool,
    ) {
        if let Some(error) = rsp_error_message(p_rsp_info) {
            if let Ok(mut shared) = self.shared.lock() {
                shared.last_error = Some(error.clone());
            }
            let _ = self.tx.send(CtpMdEvent::Error(error));
        }
    }

    fn on_rsp_sub_market_data(
        &mut self,
        p_specific_instrument: Option<&ctp2rs::v1alpha1::CThostFtdcSpecificInstrumentField>,
        p_rsp_info: Option<&CThostFtdcRspInfoField>,
        _n_request_id: i32,
        _b_is_last: bool,
    ) {
        if let Some(error) = rsp_error_message(p_rsp_info) {
            let _ = self.tx.send(CtpMdEvent::Error(error));
            return;
        }
        let instrument = p_specific_instrument
            .map(|item| item.InstrumentID.to_string())
            .unwrap_or_default();
        let _ = self.tx.send(CtpMdEvent::Subscribed(instrument));
    }

    fn on_rtn_depth_market_data(
        &mut self,
        p_depth_market_data: Option<&CThostFtdcDepthMarketDataField>,
    ) {
        let Some(depth) = p_depth_market_data else {
            let _ = self.tx.send(CtpMdEvent::Error("收到空的 depth_market_data".to_string()));
            return;
        };
        match depth_market_data_to_envelope(depth) {
            Ok(envelope) => {
                let _ = self.tx.send(CtpMdEvent::Envelope(envelope));
            }
            Err(error) => {
                let _ = self.tx.send(CtpMdEvent::Error(error));
            }
        }
    }
}

#[cfg(feature = "openctp")]
fn rsp_error_message(rsp_info: Option<&CThostFtdcRspInfoField>) -> Option<String> {
    let rsp_info = rsp_info?;
    if rsp_info.ErrorID == 0 {
        None
    } else {
        Some(format!(
            "CTP 错误 {}: {}",
            rsp_info.ErrorID,
            rsp_info.ErrorMsg.to_string()
        ))
    }
}

#[cfg(feature = "openctp")]
fn depth_market_data_to_envelope(
    depth: &CThostFtdcDepthMarketDataField,
) -> Result<MarketDataEnvelope, String> {
    let exchange_id = depth.ExchangeID.to_string();
    let instrument_id = depth.InstrumentID.to_string();
    let normalized_instrument_id = if exchange_id.is_empty() {
        instrument_id
    } else {
        format!("{}.{}", exchange_id, instrument_id)
    };

    let datetime = ctp_market_datetime(depth)?;
    let last_price = normalize_price(depth.LastPrice).unwrap_or(0.0);
    let bid_price1 = normalize_price(depth.BidPrice1).unwrap_or(last_price);
    let ask_price1 = normalize_price(depth.AskPrice1).unwrap_or(last_price);

    Ok(MarketDataEnvelope {
        source: GatewayMarketDataSource::CTP,
        replay: false,
        snapshot: MDSnapshot {
            instrument_id: normalized_instrument_id,
            amount: normalize_price(depth.Turnover).unwrap_or(0.0),
            ask_price1,
            ask_price2: normalize_price(depth.AskPrice2),
            ask_price3: normalize_price(depth.AskPrice3),
            ask_price4: normalize_price(depth.AskPrice4),
            ask_price5: normalize_price(depth.AskPrice5),
            ask_price6: None,
            ask_price7: None,
            ask_price8: None,
            ask_price9: None,
            ask_price10: None,
            ask_volume1: depth.AskVolume1 as i64,
            ask_volume2: normalize_volume(depth.AskVolume2),
            ask_volume3: normalize_volume(depth.AskVolume3),
            ask_volume4: normalize_volume(depth.AskVolume4),
            ask_volume5: normalize_volume(depth.AskVolume5),
            ask_volume6: None,
            ask_volume7: None,
            ask_volume8: None,
            ask_volume9: None,
            ask_volume10: None,
            bid_price1,
            bid_price2: normalize_price(depth.BidPrice2),
            bid_price3: normalize_price(depth.BidPrice3),
            bid_price4: normalize_price(depth.BidPrice4),
            bid_price5: normalize_price(depth.BidPrice5),
            bid_price6: None,
            bid_price7: None,
            bid_price8: None,
            bid_price9: None,
            bid_price10: None,
            bid_volume1: depth.BidVolume1 as i64,
            bid_volume2: normalize_volume(depth.BidVolume2),
            bid_volume3: normalize_volume(depth.BidVolume3),
            bid_volume4: normalize_volume(depth.BidVolume4),
            bid_volume5: normalize_volume(depth.BidVolume5),
            bid_volume6: None,
            bid_volume7: None,
            bid_volume8: None,
            bid_volume9: None,
            bid_volume10: None,
            close: optional_f64(depth.ClosePrice),
            datetime,
            highest: normalize_price(depth.HighestPrice).unwrap_or(last_price),
            last_price,
            lower_limit: normalize_price(depth.LowerLimitPrice).unwrap_or(last_price),
            lowest: normalize_price(depth.LowestPrice).unwrap_or(last_price),
            open: normalize_price(depth.OpenPrice).unwrap_or(last_price),
            open_interest: optional_f64(depth.OpenInterest),
            pre_close: normalize_price(depth.PreClosePrice).unwrap_or(last_price),
            pre_open_interest: optional_f64(depth.PreOpenInterest),
            pre_settlement: optional_f64(depth.PreSettlementPrice),
            settlement: optional_f64(depth.SettlementPrice),
            upper_limit: normalize_price(depth.UpperLimitPrice).unwrap_or(last_price),
            volume: depth.Volume as i64,
            average: normalize_price(depth.AveragePrice).unwrap_or(last_price),
            iopv: OptionalF64::Null,
        },
    })
}

#[cfg(feature = "openctp")]
fn ctp_market_datetime(
    depth: &CThostFtdcDepthMarketDataField,
) -> Result<chrono::DateTime<Utc>, String> {
    let action_day = depth.ActionDay.to_string();
    let trading_day = depth.TradingDay.to_string();
    let raw_day = if action_day.trim().is_empty() {
        trading_day
    } else {
        action_day
    };
    let date = parse_ctp_date(&raw_day)?;
    let time = parse_ctp_time(&depth.UpdateTime.to_string(), depth.UpdateMillisec)?;
    let naive = NaiveDateTime::new(date, time);
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.with_timezone(&Utc))
        .ok_or_else(|| "CTP 行情时间存在歧义".to_string())
}

#[cfg(feature = "openctp")]
fn parse_ctp_date(text: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(text.trim(), "%Y%m%d")
        .map_err(|err| format!("解析 CTP 日期失败 `{text}`: {err}"))
}

#[cfg(feature = "openctp")]
fn parse_ctp_time(text: &str, millisec: i32) -> Result<NaiveTime, String> {
    let base_time = NaiveTime::parse_from_str(text.trim(), "%H:%M:%S")
        .map_err(|err| format!("解析 CTP 时间失败 `{text}`: {err}"))?;
    let millisec = millisec.clamp(0, 999) as u32;
    base_time
        .with_nanosecond(millisec * 1_000_000)
        .ok_or_else(|| "构造 CTP 纳秒时间失败".to_string())
}

#[cfg(feature = "openctp")]
fn normalize_price(value: f64) -> Option<f64> {
    if value.is_finite() && value.abs() < 1.0e20 {
        Some(value)
    } else {
        None
    }
}

#[cfg(feature = "openctp")]
fn normalize_volume(value: i32) -> Option<i64> {
    if value >= 0 {
        Some(value as i64)
    } else {
        None
    }
}

#[cfg(feature = "openctp")]
fn optional_f64(value: f64) -> OptionalF64 {
    normalize_price(value)
        .map(OptionalF64::Value)
        .unwrap_or(OptionalF64::Null)
}
