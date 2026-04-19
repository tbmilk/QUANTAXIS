//! # QAMD 市场数据网关
//!
//! 提供统一的行情接入和实时分发能力，支持：
//! - CTP 期货行情（需 feature = "ctp"）
//! - 腾讯行情（QQ）
//! - 新浪行情（Sina）
//! - 自定义数据源（实现 `MarketDataUpdate` 消息即可接入）
//!
//! ## 架构
//! ```text
//! 数据源 (CTP/QQ/Sina/自定义)
//!     ↓  MarketDataUpdate
//! MarketDataDistributor  ← Actor，负责按订阅关系分发
//!     ↓  MarketDataUpdateMessage
//! WsSession  ← 每个 WebSocket 客户端一个实例
//!     ↓  JSON (TradingView / Legacy 格式)
//! 客户端
//! ```
//!
//! ## 快速启动
//! ```rust,no_run
//! use actix::Actor;
//! use actix_web::{web, App, HttpServer};
//! use qapro_rs::qamarket::qamdgateway::{
//!     actors::MarketDataDistributor,
//!     ws_server::ws_handler,
//! };
//!
//! #[actix_rt::main]
//! async fn main() -> std::io::Result<()> {
//!     let distributor = MarketDataDistributor::new().start();
//!     let distributor_data = web::Data::new(distributor);
//!     HttpServer::new(move || {
//!         App::new()
//!             .app_data(distributor_data.clone())
//!             .route("/ws", web::get().to(ws_handler))
//!     })
//!     .bind("0.0.0.0:8080")?
//!     .run()
//!     .await
//! }
//! ```

pub mod actors;
pub mod config;
pub mod error;
pub mod ws_server;

pub use actors::{MarketDataDistributor, PullSourcePump};
pub use actors::messages::{MarketDataSource, MarketDataUpdate, MarketDataUpdateMessage};
pub use config::Config;
pub use error::{GatewayError, GatewayResult};
pub use ws_server::ws_handler;
