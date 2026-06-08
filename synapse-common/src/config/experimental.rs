use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ExperimentalConfig {
    #[cfg(feature = "openclaw-routes")]
    #[serde(default)]
    pub openclaw_routes_enabled: bool,

    #[serde(default)]
    pub msc3814_enabled: bool,
}

#[allow(clippy::derivable_impls)]
impl Default for ExperimentalConfig {
    fn default() -> Self {
        Self {
            #[cfg(feature = "openclaw-routes")]
            openclaw_routes_enabled: true,
            msc3814_enabled: false,
        }
    }
}