//! Handlers 模块
//! 
//! 从 routes/mod.rs 提取的独立 Handler 函数

pub mod health;
pub mod versions;

pub use health::*;
pub use versions::*;