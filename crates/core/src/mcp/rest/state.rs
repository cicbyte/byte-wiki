//! REST API 共享状态

use std::sync::Arc;

use crate::AppService;

/// API 共享状态：AppService + API Key 配置
#[derive(Clone)]
pub struct ApiState {
    pub svc: Arc<AppService>,
    pub api_key: String,
    pub allow_local_no_key: bool,
}
