use std::sync::OnceLock;

static GLOBAL_ARGON2_CONFIG: OnceLock<Argon2Config> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Argon2Config {
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub output_len: Option<usize>,
}

impl Default for Argon2Config {
    fn default() -> Self {
        Self {
            m_cost: 65536,
            t_cost: 3,
            p_cost: 1,
            output_len: Some(32),
        }
    }
}

impl Argon2Config {
    pub const OWASP_MIN_M_COST: u32 = 65536;
    pub const OWASP_MIN_T_COST: u32 = 3;
    pub const OWASP_MIN_P_COST: u32 = 1;

    pub fn validate_owasp(&self) -> Result<(), String> {
        if self.m_cost < Self::OWASP_MIN_M_COST {
            return Err(format!(
                "m_cost ({}) is below OWASP minimum ({})",
                self.m_cost, Self::OWASP_MIN_M_COST
            ));
        }
        if self.t_cost < Self::OWASP_MIN_T_COST {
            return Err(format!(
                "t_cost ({}) is below OWASP minimum ({})",
                self.t_cost, Self::OWASP_MIN_T_COST
            ));
        }
        if self.p_cost < Self::OWASP_MIN_P_COST {
            return Err(format!(
                "p_cost ({}) is below OWASP minimum ({})",
                self.p_cost, Self::OWASP_MIN_P_COST
            ));
        }
        Ok(())
    }

    pub fn get_global() -> Argon2Config {
        GLOBAL_ARGON2_CONFIG.get().copied().unwrap_or_default()
    }

    pub fn initialize_global(config: Argon2Config) -> Result<(), String> {
        config.validate_owasp()?;
        let _ = GLOBAL_ARGON2_CONFIG.set(config);
        Ok(())
    }
}

fn main() {
    println!("=== Argon2 配置模块测试 ===\n");

    println!("1. 测试默认配置:");
    let default_config = Argon2Config::default();
    println!("   m_cost: {} (预期: 65536)", default_config.m_cost);
    println!("   t_cost: {} (预期: 3)", default_config.t_cost);
    println!("   p_cost: {} (预期: 1)", default_config.p_cost);
    assert_eq!(default_config.m_cost, 65536);
    assert_eq!(default_config.t_cost, 3);
    assert_eq!(default_config.p_cost, 1);
    println!("   ✓ 默认配置正确\n");

    println!("2. 测试 OWASP 验证:");
    let valid_config = Argon2Config::default();
    match valid_config.validate_owasp() {
        Ok(()) => println!("   ✓ 默认配置符合 OWASP 标准"),
        Err(e) => panic!("   ✗ 验证失败: {}", e),
    }

    let invalid_config = Argon2Config {
        m_cost: 4096,
        t_cost: 3,
        p_cost: 1,
        output_len: Some(32),
    };
    match invalid_config.validate_owasp() {
        Ok(()) => panic!("   ✗ 应该验证失败但通过了"),
        Err(e) => println!("   ✓ 正确检测到不合规配置: {}", e),
    }
    println!();

    println!("3. 测试全局配置:");
    let config = Argon2Config {
        m_cost: 65536,
        t_cost: 3,
        p_cost: 1,
        output_len: Some(32),
    };
    Argon2Config::initialize_global(config).expect("初始化全局配置失败");
    let global = Argon2Config::get_global();
    println!("   全局配置 m_cost: {} (预期: 65536)", global.m_cost);
    assert_eq!(global.m_cost, 65536);
    println!("   ✓ 全局配置正确\n");

    println!("4. 测试 OWASP 常量:");
    println!("   OWASP_MIN_M_COST: {}", Argon2Config::OWASP_MIN_M_COST);
    println!("   OWASP_MIN_T_COST: {}", Argon2Config::OWASP_MIN_T_COST);
    println!("   OWASP_MIN_P_COST: {}", Argon2Config::OWASP_MIN_P_COST);
    assert_eq!(Argon2Config::OWASP_MIN_M_COST, 65536);
    assert_eq!(Argon2Config::OWASP_MIN_T_COST, 3);
    assert_eq!(Argon2Config::OWASP_MIN_P_COST, 1);
    println!("   ✓ OWASP 常量正确\n");

    println!("=== 所有测试通过! ===");
}
