//! Handlers 模块
//! 
//! 从 routes/mod.rs 提取的独立 Handler 函数

pub mod health;
pub mod versions;
pub mod auth;
pub mod user;
pub mod room;

pub use health::*;
pub use versions::*;
pub use auth::*;
pub use user::*;
pub use room::*;