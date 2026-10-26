//! `crate::common::error::*` 路径的稳定入口（facade）。
//!
//! 真实定义在 [`synapse_common::error`]；这里保留模块路径是因为仓内外有大量
//! `synapse_rust::common::error::{ApiError, MatrixErrorCode, …}` 的写法
//! （含 `tests/integration/*`），删掉模块会连带改动几十处调用点。
//!
//! 2026-10-26：原先这里还挂着 `crypto_error_to_api_error` /
//! `ed25519_error_to_api_error` 两个转换 helper。全仓 `grep` 显示它们除了本文件
//! 的定义与 `common/mod.rs` 的 re-export 之外**零调用点**（`synapse-e2ee` 里那几个
//! `test_crypto_error_to_api_error_*` 测的是它自己 crate 的函数，与本文件无关），
//! 因此已删除 —— 留着只会让人以为根 crate 有一层"统一错误转换"的约定。
pub use synapse_common::error::{init_error_metrics, ApiError, ApiErrorKind, ApiResponse, ApiResult, MatrixErrorCode};
