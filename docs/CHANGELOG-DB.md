# Database Migration Changelog

> 本文件记录所有迁移脚本的删除历史，包括归档备份信息。
> 所有删除操作必须先标记 deprecated，保留一个发布周期后才能物理删除。

---

## 2026-03-29

### 初始版本

- 本文件建立，用于记录迁移脚本的生命周期变更

---

## 删除记录模板

```markdown
## YYYY-MM-DD - 删除: {filename}

- **Version**: V{version}
- **Jira**: {jira}
- **Description**: {description}
- **Reason**: {reason}
- **Approved by**: {reviewers}
- **Git Archive**: {archive_tag}
- **Archive File**: {archive_path}
```

---

## 归档恢复指南

### 从 Git Tag 恢复

```bash
# 查看归档 tag
git tag -l "archive/*"

# 检出归档文件
git checkout archive/{version}__{jira} -- migrations/{filename}

# 或提取归档文件
git archive archive/{version}__{jira} | tar -xf -
```

### 从备份文件恢复

```bash
# 解压归档
tar -xzf {archive_name}.tar.gz

# 恢复文件
cp {archive_name}/migrations/{filename} migrations/
```
