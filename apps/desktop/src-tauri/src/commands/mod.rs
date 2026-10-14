//! Tauri 命令按业务域拆分（每个命令是 core AppService 的薄封装）

pub mod system;
pub mod wallpaper;
pub mod config;
pub mod llm;
pub mod repo;
pub mod generate;
pub mod llm_logs;
pub mod callchain;
pub mod watch;
pub mod cards;
pub mod servers;
