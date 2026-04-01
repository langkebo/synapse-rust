use std::path::PathBuf;
use std::process::Command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docker")
        .join("db_migrate.sh");

    if !script_path.exists() {
        return Err(format!("迁移入口不存在: {}", script_path.display()).into());
    }

    println!("🔧 run_migrations 已降级为兼容包装器");
    println!("📌 迁移主链以 docker/db_migrate.sh 与 db-migration-gate.yml 为准");
    println!("🚀 正在委托执行: {} migrate", script_path.display());

    let status = Command::new("bash")
        .arg(&script_path)
        .arg("migrate")
        .status()?;

    if !status.success() {
        return Err(format!("db_migrate.sh 执行失败，退出码: {:?}", status.code()).into());
    }

    println!("✅ 统一迁移入口执行完成");
    Ok(())
}
