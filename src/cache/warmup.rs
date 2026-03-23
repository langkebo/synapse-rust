//! 缓存预热模块
//!
//! 提供启动时缓存预热和定时预热功能

use crate::cache::CacheManager;
use crate::storage::Pool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::interval;

/// 预热配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupConfig {
    /// 是否启用启动预热
    pub enabled: bool,
    /// 启动预热延迟（秒）
    pub startup_delay_secs: u64,
    /// 定时预热间隔（秒）
    pub interval_secs: u64,
    /// 预热的用户数上限
    pub max_users: usize,
    /// 预热的房间数上限
    pub max_rooms: usize,
}

impl Default for WarmupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            startup_delay_secs: 10,    // 10 seconds (was 5s) - allow more time for startup
            interval_secs: 900,        // 15 minutes (was 5 min) - reduce frequency
            max_users: 500,            // 500 users (was 100) - warm more popular users
            max_rooms: 200,            // 200 rooms (was 50) - warm more popular rooms
        }
    }
}

/// 预热任务类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarmupTask {
    /// 热门用户
    PopularUsers,
    /// 热门房间
    PopularRooms,
    /// 房间成员
    RoomMembers,
    /// 房间状态
    RoomState,
    /// 用户配置
    UserSettings,
}

impl WarmupTask {
    pub fn name(&self) -> &'static str {
        match self {
            WarmupTask::PopularUsers => "popular_users",
            WarmupTask::PopularRooms => "popular_rooms",
            WarmupTask::RoomMembers => "room_members",
            WarmupTask::RoomState => "room_state",
            WarmupTask::UserSettings => "user_settings",
        }
    }
}

/// 预热状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupStatus {
    pub last_run: Option<i64>,
    pub next_run: Option<i64>,
    pub tasks_completed: usize,
    pub tasks_failed: usize,
    pub items_warmed: usize,
}

impl Default for WarmupStatus {
    fn default() -> Self {
        Self {
            last_run: None,
            next_run: None,
            tasks_completed: 0,
            tasks_failed: 0,
            items_warmed: 0,
        }
    }
}

/// 缓存预热器
pub struct CacheWarmer {
    config: WarmupConfig,
    cache: Arc<CacheManager>,
    db_pool: Pool,
    status: Arc<RwLock<WarmupStatus>>,
    running: Arc<RwLock<bool>>,
}

impl CacheWarmer {
    pub fn new(config: WarmupConfig, cache: Arc<CacheManager>, db_pool: Pool) -> Self {
        Self {
            config,
            cache,
            db_pool,
            status: Arc::new(RwLock::new(WarmupStatus::default())),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// 启动预热任务
    pub async fn start(&self) {
        if !self.config.enabled {
            tracing::info!("Cache warmer is disabled");
            return;
        }

        let mut running = self.running.write().await;
        if *running {
            tracing::warn!("Cache warmer is already running");
            return;
        }
        *running = true;
        drop(running);

        let config = self.config.clone();
        let cache = self.cache.clone();
        let db_pool = self.db_pool.clone();
        let status = self.status.clone();
        let running = self.running.clone();

        // 启动延迟后执行预热
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(config.startup_delay_secs)).await;
            
            tracing::info!("Starting cache warmup...");
            
            // 执行启动预热
            Self::run_warmup(&config, &cache, &db_pool, &status).await;

            // 启动定时预热循环
            let mut interval = interval(Duration::from_secs(config.interval_secs));
            loop {
                interval.tick().await;
                
                let is_running = running.read().await;
                if !*is_running {
                    break;
                }
                drop(is_running);

                tracing::debug!("Running scheduled cache warmup...");
                Self::run_warmup(&config, &cache, &db_pool, &status).await;
            }
            
            tracing::info!("Cache warmer stopped");
        });
    }

    /// 停止预热任务
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
    }

    /// 手动触发预热
    pub async fn trigger_warmup(&self) {
        tracing::info!("Manual cache warmup triggered");
        Self::run_warmup(
            &self.config,
            &self.cache,
            &self.db_pool,
            &self.status,
        ).await;
    }

    /// 获取预热状态
    pub async fn get_status(&self) -> WarmupStatus {
        self.status.read().await.clone()
    }

    /// 执行预热
    async fn run_warmup(
        config: &WarmupConfig,
        cache: &CacheManager,
        db_pool: &Pool,
        status: &Arc<RwLock<WarmupStatus>>,
    ) {
        let mut status_guard = status.write().await;
        status_guard.last_run = Some(chrono::Utc::now().timestamp_millis());
        
        let tasks = [
            WarmupTask::PopularUsers,
            WarmupTask::PopularRooms,
            WarmupTask::UserSettings,
        ];

        for task in tasks {
            match Self::execute_task(task, config, cache, db_pool).await {
                Ok(count) => {
                    status_guard.tasks_completed += 1;
                    status_guard.items_warmed += count;
                    tracing::info!("Warmup task {} completed: {} items", task.name(), count);
                }
                Err(e) => {
                    status_guard.tasks_failed += 1;
                    tracing::error!("Warmup task {} failed: {}", task.name(), e);
                }
            }
        }
    }

    /// 执行单个预热任务
    async fn execute_task(
        task: WarmupTask,
        config: &WarmupConfig,
        cache: &CacheManager,
        db_pool: &Pool,
    ) -> Result<usize, String> {
        match task {
            WarmupTask::PopularUsers => {
                Self::warmup_popular_users(cache, db_pool, config.max_users).await
            }
            WarmupTask::PopularRooms => {
                Self::warmup_popular_rooms(cache, db_pool, config.max_rooms).await
            }
            WarmupTask::UserSettings => {
                Self::warmup_user_settings(cache, db_pool, config.max_users).await
            }
            _ => Ok(0), // 其他任务暂未实现
        }
    }

    /// 预热热门用户
    async fn warmup_popular_users(
        cache: &CacheManager,
        db_pool: &Pool,
        limit: usize,
    ) -> Result<usize, String> {
        let rows = sqlx::query!(
            r#"
            SELECT user_id, username, displayname, avatar_url 
            FROM users 
            ORDER BY created_ts DESC 
            LIMIT $1
            "#,
            limit as i64
        )
        .fetch_all(db_pool.as_ref())
        .await
        .map_err(|e| e.to_string())?;

        for row in &rows {
            let key = format!("user:profile:{}", row.user_id);
            if let Ok(value) = serde_json::json!({
                "user_id": row.user_id,
                "username": row.username,
                "displayname": row.displayname,
                "avatar_url": row.avatar_url,
            }).to_string() {
                cache.set_raw(&key, &value);
            }
        }

        Ok(rows.len())
    }

    /// 预热热门房间
    async fn warmup_popular_rooms(
        cache: &CacheManager,
        db_pool: &Pool,
        limit: usize,
    ) -> Result<usize, String> {
        let rows = sqlx::query!(
            r#"
            SELECT room_id, name, topic, avatar_url, member_count
            FROM rooms 
            ORDER BY created_ts DESC 
            LIMIT $1
            "#,
            limit as i64
        )
        .fetch_all(db_pool.as_ref())
        .await
        .map_err(|e| e.to_string())?;

        for row in &rows {
            let key = format!("room:info:{}", row.room_id);
            if let Ok(value) = serde_json::json!({
                "room_id": row.room_id,
                "name": row.name,
                "topic": row.topic,
                "avatar_url": row.avatar_url,
                "member_count": row.member_count,
            }).to_string() {
                cache.set_raw(&key, &value);
            }
        }

        Ok(rows.len())
    }

    /// 预热用户设置
    async fn warmup_user_settings(
        cache: &CacheManager,
        db_pool: &Pool,
        limit: usize,
    ) -> Result<usize, String> {
        let rows = sqlx::query!(
            r#"
            SELECT user_id, theme, language, time_zone
            FROM user_settings 
            LIMIT $1
            "#,
            limit as i64
        )
        .fetch_all(db_pool.as_ref())
        .await
        .map_err(|e| e.to_string())?;

        for row in &rows {
            let key = format!("user:settings:{}", row.user_id);
            if let Ok(value) = serde_json::json!({
                "user_id": row.user_id,
                "theme": row.theme,
                "language": row.language,
                "time_zone": row.time_zone,
            }).to_string() {
                cache.set_raw(&key, &value);
            }
        }

        Ok(rows.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warmup_config_default() {
        let config = WarmupConfig::default();
        assert!(config.enabled);
        assert_eq!(config.startup_delay_secs, 10);
        assert_eq!(config.interval_secs, 900);
        assert_eq!(config.max_users, 500);
        assert_eq!(config.max_rooms, 200);
    }

    #[test]
    fn test_warmup_task_name() {
        assert_eq!(WarmupTask::PopularUsers.name(), "popular_users");
        assert_eq!(WarmupTask::PopularRooms.name(), "popular_rooms");
        assert_eq!(WarmupTask::RoomMembers.name(), "room_members");
        assert_eq!(WarmupTask::RoomState.name(), "room_state");
        assert_eq!(WarmupTask::UserSettings.name(), "user_settings");
    }

    #[test]
    fn test_warmup_status_default() {
        let status = WarmupStatus::default();
        assert!(status.last_run.is_none());
        assert!(status.next_run.is_none());
        assert_eq!(status.tasks_completed, 0);
        assert_eq!(status.tasks_failed, 0);
        assert_eq!(status.items_warmed, 0);
    }

    #[test]
    fn test_warmup_config_disabled() {
        let config = WarmupConfig {
            enabled: false,
            startup_delay_secs: 5,
            interval_secs: 300,
            max_users: 100,
            max_rooms: 50,
        };
        assert!(!config.enabled);
    }

    #[test]
    fn test_warmup_config_custom_values() {
        let config = WarmupConfig {
            enabled: true,
            startup_delay_secs: 30,
            interval_secs: 1800,
            max_users: 1000,
            max_rooms: 500,
        };
        assert_eq!(config.startup_delay_secs, 30);
        assert_eq!(config.interval_secs, 1800);
        assert_eq!(config.max_users, 1000);
        assert_eq!(config.max_rooms, 500);
    }

    #[test]
    fn test_warmup_task_all_variants() {
        let tasks = vec![
            WarmupTask::PopularUsers,
            WarmupTask::PopularRooms,
            WarmupTask::RoomMembers,
            WarmupTask::RoomState,
            WarmupTask::UserSettings,
        ];
        
        for task in tasks {
            assert!(!task.name().is_empty());
        }
    }

    #[test]
    fn test_warmup_status_update() {
        let mut status = WarmupStatus::default();
        
        status.last_run = Some(1234567890);
        status.next_run = Some(1234567890 + 3600);
        status.tasks_completed = 5;
        status.tasks_failed = 1;
        status.items_warmed = 100;
        
        assert_eq!(status.last_run, Some(1234567890));
        assert_eq!(status.next_run, Some(1234567890 + 3600));
        assert_eq!(status.tasks_completed, 5);
        assert_eq!(status.tasks_failed, 1);
        assert_eq!(status.items_warmed, 100);
    }
}
