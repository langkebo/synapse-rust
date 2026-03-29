//! Handlers 模块
//!
//! 从 routes/mod.rs 提取的独立 Handler 函数

pub mod auth;
pub mod health;
pub mod presence;
pub mod room;
pub mod sync;
pub mod user;
pub mod versions;

pub use auth::*;
pub use health::*;
pub use user::*;
pub use versions::*;
