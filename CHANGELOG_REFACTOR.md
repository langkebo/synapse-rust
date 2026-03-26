# 变更日志 (Changelog)

## [Unreleased] - 2026-03-24
### Added
- 增加了 `clippy.toml` 配置文件，用于定义合理的代码复杂度指标（`too-many-lines-threshold` 和 `cognitive-complexity-threshold`）。
- 增加了自动化脚本以检查项目中的长函数，并实施了必要的调整。

### Changed
- **代码重构与质量提升**:
  - 修复了 `builtin_oidc_provider.rs` 中的无效借用编译警告。
  - 修复了 `search_service.rs` 中的无必要类型转换警告。
  - 修复了集成测试 `api_room_tests.rs` 和 `transaction_tests.rs` 中因配置项（`SearchConfig`, `Config`）变更而导致的编译失败问题。
  - 修复了 `src/common/config.rs` 测试块中的缺漏字段问题。
  - 使用 `cargo fmt` 对整个项目进行了全面的代码格式化，统一了官方编码风格。
- **Docker 优化**:
  - 删除了根目录下冗余的 `Dockerfile`、`docker-compose.yml`、`docker-compose.prod.yml` 等文件，统一使用 `docker` 目录内的配置。
  - 验证了 `docker/Dockerfile` 采用的多阶段构建（Multi-stage Build），显著减少了最终运行时的镜像体积。
- **依赖优化**:
  - 使用 `cargo-machete` 分析并移除了12个未使用的依赖项（包括 `deadpool`, `elasticsearch`, `hyper`, `tokio-util`, `urlencoding` 等），减小了编译和运行时的负担。
  - 移除了 15.0GiB 的冗余构建产物。

### Removed
- 删除了废弃的 Docker 配置文件和未使用的 Cargo 依赖项。

### Fixed
- 修复了所有 `cargo clippy` 报告的关键安全和代码规范警告。
- 确保所有功能测试、集成测试无缝通过。