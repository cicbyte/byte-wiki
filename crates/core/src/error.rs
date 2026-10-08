//! 核心错误类型

use thiserror::Error;

/// 核心库统一错误
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("数据库错误: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("项目不存在: {0}")]
    NotFound(String),

    #[error("项目已存在: {0}")]
    AlreadyExists(String),

    #[error("路径不存在: {0}")]
    PathNotFound(String),

    #[error("shortId 生成重试 {0} 次后仍碰撞")]
    ShortIdCollision(usize),

    #[error("无效参数: {0}")]
    InvalidArg(String),

    #[error("序列化错误: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("遍历目录错误: {0}")]
    Walkdir(#[from] walkdir::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type CoreResult<T> = Result<T, CoreError>;
