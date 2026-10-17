use serde::{Deserialize, Deserializer};

/// 后台任务调度器并发上限的默认值，对齐上游 `synapse/config/task_scheduler.py`
/// 的 `max_concurrent_tasks = 5`（上游 #20230）。
pub const DEFAULT_MAX_CONCURRENT_TASKS: usize = 5;

/// 后台任务调度器配置（对应上游 `synapse/config/task_scheduler.py`）。
///
/// 上游 #20230 新增 `task_scheduler.max_concurrent_tasks`，默认 `5`，并为它写了
/// **显式分支拒绝布尔值**：在弱类型语言里 YAML 的 `true` 会被当成 `1` 悄悄生效。
/// 本仓用自定义反序列化（见 [`deserialize_max_concurrent_tasks`]）把「能反序列化成
/// 整数、但语义是布尔」的输入一并挡掉，同时拒绝 `0`、负数、非整数与字符串。
#[derive(Debug, Clone, Deserialize)]
/// Represents TaskSchedulerConfig.
pub struct TaskSchedulerConfig {
    /// 后台任务调度器的最大并发数，必须是**正整数**。
    ///
    /// 非法输入（布尔、`0`、负数、非整数、字符串）在配置加载期直接报错，而不是
    /// 静默回退到默认值 —— 静默回退会让"配了但不生效"无从察觉。
    #[serde(default = "default_max_concurrent_tasks", deserialize_with = "deserialize_max_concurrent_tasks")]
    /// `max_concurrent_tasks` field.
    pub max_concurrent_tasks: usize,
}

impl Default for TaskSchedulerConfig {
    fn default() -> Self {
        Self { max_concurrent_tasks: default_max_concurrent_tasks() }
    }
}

fn default_max_concurrent_tasks() -> usize {
    DEFAULT_MAX_CONCURRENT_TASKS
}

/// 反序列化 `max_concurrent_tasks`：只接受正整数，拒绝布尔与非正整数（上游 #20230）。
fn deserialize_max_concurrent_tasks<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error as _;

    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Number(number) => match number.as_u64().and_then(|value| usize::try_from(value).ok()) {
            Some(value) if value > 0 => Ok(value),
            _ => Err(D::Error::custom(format!(
                "task_scheduler.max_concurrent_tasks must be a positive integer, got {number}"
            ))),
        },
        serde_json::Value::Bool(value) => Err(D::Error::custom(format!(
            "task_scheduler.max_concurrent_tasks must be an integer, not a boolean ({value})"
        ))),
        other => Err(D::Error::custom(format!(
            "task_scheduler.max_concurrent_tasks must be a positive integer, got {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_scheduler_config_default() {
        assert_eq!(TaskSchedulerConfig::default().max_concurrent_tasks, 5);
        assert_eq!(default_max_concurrent_tasks(), 5);
    }

    #[test]
    fn test_empty_yaml_uses_default() {
        let cfg: TaskSchedulerConfig = serde_norway::from_str("{}").expect("empty mapping should default");
        assert_eq!(cfg.max_concurrent_tasks, 5);
    }

    #[test]
    fn test_positive_integer_is_accepted() {
        let cfg: TaskSchedulerConfig =
            serde_norway::from_str("max_concurrent_tasks: 12").expect("positive integer should be accepted");
        assert_eq!(cfg.max_concurrent_tasks, 12);
    }

    /// 负控：YAML 的 `true` 能被弱类型语言当成 `1` 用；必须被显式拒绝。
    #[test]
    fn test_boolean_is_rejected() {
        let result: Result<TaskSchedulerConfig, _> = serde_norway::from_str("max_concurrent_tasks: true");
        let error = result.expect_err("boolean must be rejected").to_string();
        assert!(error.contains("not a boolean"), "unexpected error: {error}");
    }

    #[test]
    fn test_zero_is_rejected() {
        let result: Result<TaskSchedulerConfig, _> = serde_norway::from_str("max_concurrent_tasks: 0");
        let error = result.expect_err("zero must be rejected").to_string();
        assert!(error.contains("positive integer"), "unexpected error: {error}");
    }

    #[test]
    fn test_negative_is_rejected() {
        let result: Result<TaskSchedulerConfig, _> = serde_norway::from_str("max_concurrent_tasks: -3");
        let error = result.expect_err("negative must be rejected").to_string();
        assert!(error.contains("positive integer"), "unexpected error: {error}");
    }

    #[test]
    fn test_non_integer_is_rejected() {
        for yaml in ["max_concurrent_tasks: 1.5", "max_concurrent_tasks: \"8\"", "max_concurrent_tasks: null"] {
            let result: Result<TaskSchedulerConfig, _> = serde_norway::from_str(yaml);
            assert!(result.is_err(), "{yaml} must be rejected");
        }
    }
}
