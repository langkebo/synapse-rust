use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FeatureFlags {
    pub room_summary: RoomSummaryFlags,
    pub dm: DmFlags,
    pub space: SpaceFlags,
    pub pushers: PusherFlags,
    pub verification: VerificationFlags,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoomSummaryFlags {
    #[serde(default = "default_realtime_sync")]
    pub realtime_sync: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DmFlags {
    #[serde(default = "default_stable_mode")]
    pub stable_mode: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpaceFlags {
    #[serde(default = "default_max_depth")]
    pub max_depth: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PusherFlags {
    #[serde(default)]
    pub experimental: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VerificationFlags {
    #[serde(default)]
    pub use_new_api: bool,
}

fn default_realtime_sync() -> bool {
    false
}

fn default_stable_mode() -> bool {
    false
}

fn default_max_depth() -> i32 {
    100
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            room_summary: RoomSummaryFlags::default(),
            dm: DmFlags::default(),
            space: SpaceFlags::default(),
            pushers: PusherFlags::default(),
            verification: VerificationFlags::default(),
        }
    }
}

impl Default for RoomSummaryFlags {
    fn default() -> Self {
        Self {
            realtime_sync: default_realtime_sync(),
        }
    }
}

impl Default for DmFlags {
    fn default() -> Self {
        Self {
            stable_mode: default_stable_mode(),
        }
    }
}

impl Default for SpaceFlags {
    fn default() -> Self {
        Self {
            max_depth: default_max_depth(),
        }
    }
}

impl Default for PusherFlags {
    fn default() -> Self {
        Self {
            experimental: false,
        }
    }
}

impl Default for VerificationFlags {
    fn default() -> Self {
        Self {
            use_new_api: false,
        }
    }
}

#[derive(Clone)]
pub struct FeatureFlagService {
    flags: Arc<RwLock<FeatureFlags>>,
}

impl FeatureFlagService {
    pub fn new() -> Self {
        Self {
            flags: Arc::new(RwLock::new(FeatureFlags::default())),
        }
    }

    pub async fn update(&self, flags: FeatureFlags) {
        let mut current = self.flags.write().await;
        *current = flags;
    }

    pub async fn get_flags(&self) -> FeatureFlags {
        self.flags.read().await.clone()
    }

    pub async fn is_room_summary_realtime_sync_enabled(&self) -> bool {
        self.flags.read().await.room_summary.realtime_sync
    }

    pub async fn is_dm_stable_mode_enabled(&self) -> bool {
        self.flags.read().await.dm.stable_mode
    }

    pub async fn get_space_max_depth(&self) -> i32 {
        self.flags.read().await.space.max_depth
    }

    pub async fn is_pusher_experimental_enabled(&self) -> bool {
        self.flags.read().await.pushers.experimental
    }

    pub async fn is_verification_new_api_enabled(&self) -> bool {
        self.flags.read().await.verification.use_new_api
    }
}

impl Default for FeatureFlagService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_feature_flags_default() {
        let service = FeatureFlagService::new();
        let flags = service.get_flags().await;

        assert!(!flags.room_summary.realtime_sync);
        assert!(!flags.dm.stable_mode);
        assert_eq!(flags.space.max_depth, 100);
        assert!(!flags.pushers.experimental);
        assert!(!flags.verification.use_new_api);
    }

    #[tokio::test]
    async fn test_update_flags() {
        let service = FeatureFlagService::new();

        let new_flags = FeatureFlags {
            room_summary: RoomSummaryFlags {
                realtime_sync: true,
            },
            dm: DmFlags {
                stable_mode: true,
            },
            space: SpaceFlags {
                max_depth: 50,
            },
            pushers: PusherFlags {
                experimental: true,
            },
            verification: VerificationFlags {
                use_new_api: true,
            },
        };

        service.update(new_flags.clone()).await;
        let flags = service.get_flags().await;

        assert!(flags.room_summary.realtime_sync);
        assert!(flags.dm.stable_mode);
        assert_eq!(flags.space.max_depth, 50);
        assert!(flags.pushers.experimental);
        assert!(flags.verification.use_new_api);
    }

    #[tokio::test]
    async fn test_individual_flag_checks() {
        let service = FeatureFlagService::new();

        assert!(!service.is_room_summary_realtime_sync_enabled().await);
        assert!(!service.is_dm_stable_mode_enabled().await);
        assert_eq!(service.get_space_max_depth().await, 100);
        assert!(!service.is_pusher_experimental_enabled().await);
        assert!(!service.is_verification_new_api_enabled().await);

        let flags = FeatureFlags {
            room_summary: RoomSummaryFlags {
                realtime_sync: true,
            },
            dm: DmFlags::default(),
            space: SpaceFlags::default(),
            pushers: PusherFlags::default(),
            verification: VerificationFlags::default(),
        };

        service.update(flags).await;

        assert!(service.is_room_summary_realtime_sync_enabled().await);
    }
}
