use serde::{Deserialize, Deserializer};

/// 后台任务调度器并发上限的默认值，对齐上游 `synapse/config/task_scheduler.py`
/// 的 `max_concurrent_tasks = 5`（上游 #20230）。
pub const DEFAULT_MAX_CONCURRENT_TASKS: usize = 5;

/// 后台任务调度器配置（对应上游 `synapse/config/task_scheduler.py`）。
///
/// 上游 #20230 新增 `task_scheduler.max_concurrent_tasks`，默认 `5`。本仓只有一个消费点
/// （`wiring/admin.rs` 把它接到 AS 调度器每 tick 的派发上限），但**必须拒绝 `0`**：
/// 0 会让调度器每 tick 派发 0 个服务，表现为"应用服务永不运行"却不报任何错。
///
/// 其余非法输入交给 serde：`true`（YAML 里在弱类型语言会被当成 `1`）、负数、浮点、
/// 字符串、`null` 都反序列化不成 `u64`。因此自定义反序列化只补 serde 做不到的两点：
/// 校验 `> 0`、以及 `usize` 装得下（32 位目标上 `u64` 可能超界）。
#[derive(Debug, Clone, Deserialize)]
/// Represents TaskSchedulerConfig.
pub struct TaskSchedulerConfig {
    /// 后台任务调度器的最大并发数，必须是**正整数**。
    ///
    /// 非法输入（`0`、布尔、负数、非整数、字符串）在配置加载期直接报错，而不是
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

/// 反序列化 `max_concurrent_tasks`：接受 `u64`，但只保留正整数（上游 #20230）。
///
/// 类型先落到 `u64` 而不是 `usize`：`usize` 在 32 位目标上只有 32 位，超界输入会得到
/// serde 的 "invalid value" 而不是我们自己的错误；先收 `u64` 再 `try_from`，越界就能
/// 说清是哪一边越界。`0` 是唯一 serde 挡不住的非法值（它是个合法整数），所以由这里拒。
fn deserialize_max_concurrent_tasks<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error as _;

    let raw = u64::deserialize(deserializer)?;
    let value = usize::try_from(raw).map_err(|_| {
        D::Error::custom(format!("task_scheduler.max_concurrent_tasks is too large for this platform, got {raw}"))
    })?;
    if value == 0 {
        return Err(D::Error::custom("task_scheduler.max_concurrent_tasks must be a positive integer, got 0"));
    }
    Ok(value)
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

    /// 负控：`0` 是合法整数，serde 不会拦；放行等于"应用服务永不运行"。
    #[test]
    fn test_zero_is_rejected() {
        let result: Result<TaskSchedulerConfig, _> = serde_norway::from_str("max_concurrent_tasks: 0");
        let error = result.expect_err("zero must be rejected").to_string();
        assert!(error.contains("positive integer"), "unexpected error: {error}");
    }

    /// 这张表是"自定义反序列化只需要校验 `> 0`"的依据：其余非法输入 serde 自己就拒。
    ///
    /// 其中 `true` 那条最关键 —— 上游 #20230 专门为它写了显式分支，因为 YAML 在弱类型
    /// 语言里会把 `true` 当成 `1`。这里断言的是**被拒**这件事本身，而不是某句错误文案：
    /// 真正的回归是"`true` 静默变成 1"，任何 `is_err()` 失败都能抓住它。
    #[test]
    fn test_invalid_inputs_are_rejected() {
        for yaml in [
            "max_concurrent_tasks: true",
            "max_concurrent_tasks: -3",
            "max_concurrent_tasks: 1.5",
            "max_concurrent_tasks: \"8\"",
            "max_concurrent_tasks: null",
            "max_concurrent_tasks: [5]",
        ] {
            let result: Result<TaskSchedulerConfig, _> = serde_norway::from_str(yaml);
            let error = result.err().unwrap_or_else(|| panic!("{yaml} must be rejected"));
            // 错误必须能定位到字段，否则运维拿到一句无上下文的解析错误。
            assert!(error.to_string().contains("max_concurrent_tasks"), "{yaml}: {error}");
        }
    }
}
