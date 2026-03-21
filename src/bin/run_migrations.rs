use sqlx::postgres::PgPoolOptions;
use std::env;
use std::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔧 开始执行数据库迁移...\n");

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string());

    println!("📡 连接数据库: {}", database_url);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    println!("✅ 数据库连接成功\n");

    let migration_sql = fs::read_to_string("migrations/20260308000005_fix_test_failures.sql")
        .expect("无法读取迁移文件");

    println!("📄 执行迁移脚本...\n");

    let statements: Vec<&str> = migration_sql
        .split(';')
        .filter(|s| !s.trim().is_empty() && !s.trim().starts_with("--"))
        .collect();

    let total = statements.len();
    let mut success = 0;
    let mut failed = 0;

    for (i, statement) in statements.iter().enumerate() {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }

        print!(
            "[{}/{}] 执行: {:.60}... ",
            i + 1,
            total,
            trimmed.lines().next().unwrap_or("").trim()
        );

        match sqlx::query(trimmed).execute(&pool).await {
            Ok(_) => {
                println!("✅");
                success += 1;
            }
            Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("already exists")
                    || error_msg.contains("duplicate key")
                    || error_msg.contains("relation") && error_msg.contains("already exists")
                {
                    println!("⚠️  (已存在)");
                    success += 1;
                } else {
                    println!("❌");
                    println!("   错误: {}", error_msg);
                    failed += 1;
                }
            }
        }
    }

    println!("\n📊 迁移结果:");
    println!("  ✅ 成功: {}", success);
    println!("  ❌ 失败: {}", failed);
    println!("  📝 总计: {}", total);

    if failed == 0 {
        println!("\n🎉 所有迁移已成功执行！");
    } else {
        println!("\n⚠️  部分迁移失败，请检查错误信息");
    }

    Ok(())
}
