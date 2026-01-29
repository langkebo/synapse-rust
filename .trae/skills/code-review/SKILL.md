---
name: "code-review"
description: "Reviews Rust code for security, performance, maintainability, and best practices based on project rules. Invoke when code is completed or user requests code review."
---

# Rust Code Review Skill

This skill reviews Rust code according to the project's code quality guidelines and best practices.

## When to Invoke

- After completing significant code changes or new features
- When user explicitly requests code review
- Before merging pull requests
- When refactoring existing code

## Review Checklist

### 1. Security Review (安全审查)

Check for common security vulnerabilities:

- [ ] **SQL Injection**: Verify all database queries use parameterized queries
- [ ] **Input Validation**: Check boundary checks and input sanitization
- [ ] **Unsafe Code**: Review all `unsafe` blocks for necessity and correctness
- [ ] **Secret Management**: Ensure no hardcoded secrets or credentials
- [ ] **Authentication**: Verify token validation and authorization checks
- [ ] **Data Exposure**: Check for sensitive data in logs or error messages

### 2. Performance Review (性能审查)

Identify performance bottlenecks:

- [ ] **Algorithm Complexity**: Look for O(n²) or worse algorithms
- [ ] **Memory Usage**: Check for unnecessary clones and allocations
- [ ] **Database Queries**: Identify N+1 query problems
- [ ] **Caching**: Verify appropriate use of caching (local/Redis)
- [ ] **Async/Await**: Check for blocking operations in async contexts
- [ ] **Collection Types**: Ensure optimal collection choices (Vec vs HashMap, etc.)

### 3. Maintainability Review (可维护性审查)

Evaluate code quality and maintainability:

- [ ] **Function Complexity**: Functions should be under 30 lines
- [ ] **Nesting Depth**: Maximum 1 level of nesting (use early returns)
- [ ] **DRY Principle**: No code duplication
- [ ] **Naming**: Clear, descriptive names (no abbreviations)
- [ **Comments**: Documentation comments (`///`) for public APIs
- [ ] **Testing**: Unit tests with `#[cfg(test)]` for each module
- [ ] **Error Handling**: Proper use of `Result` types, not panics

### 4. Rust Best Practices (Rust 最佳实践)

Verify adherence to Rust idioms:

- [ ] **Ownership**: Proper use of references and lifetimes
- [ ] **Error Handling**: Use `Result` instead of `panic!`/`unwrap()`
- [ ] **Pattern Matching**: Use `match` instead of `if-else` chains
- [ ] **Newtype Pattern**: Wrap primitive types for type safety
- [ ] **Method Chaining**: One `.` per line (avoid long chains)
- [ ] **Builder Pattern**: Use for complex object construction
- [ ] **Trait Bounds**: Appropriate use of generic constraints
- [ ] **Concurrency**: Proper use of `Send` + `Sync` traits

### 5. Project-Specific Rules (项目特定规则)

Based on the "Nine Commandments" adapted for Rust:

- [ ] **Indentation**: Only one level of indentation per method
- [ ] **No Else**: Use `match` or early returns instead
- [ ] **Encapsulation**: Wrap primitive types (Newtype pattern)
- [ ] **Dot Operator**: One `.` per line maximum
- [ ] **Full Names**: No abbreviations in variable/function names
- [ ] **Simple Entities**: Structs with ≤5 fields, use nested structs
- [ ] **Instance Variables**: ≤2 fields per struct (group related fields)
- [ ] **First-Class Collections**: Encapsulate collections with behavior
- [ **No Getters/Setters**: Use behavior-oriented methods

## Output Format

Provide review results in the following structure:

### 📋 Summary
Brief overview of the review findings

### ⚠️ Critical Issues
Issues that must be fixed immediately:
- **Issue**: Description
  - **Location**: [file path](file:///absolute/path/to/file#L123)
  - **Severity**: Critical
  - **Recommendation**: Specific fix with code example

### 🔶 High Priority Issues
Important issues that should be addressed soon:
- **Issue**: Description
  - **Location**: [file path](file:///absolute/path/to/file#L456)
  - **Severity**: High
  - **Recommendation**: Specific fix with code example

### 💡 Suggestions
Improvements for better code quality:
- **Suggestion**: Description
  - **Location**: [file path](file:///absolute/path/to/file#L789)
  - **Recommendation**: Specific improvement with code example

### ✅ Positive Aspects
Highlight good practices found in the code

## Tool Usage

- **Read**: Read relevant files to understand context
- **Grep**: Search for similar patterns across the codebase
- **SearchCodebase**: Find related implementations
- **DO NOT modify code**: Only provide suggestions and recommendations

## Code Examples

### Example Review Output

```markdown
### 📋 Summary
Reviewed 3 files, found 2 critical issues, 3 high priority issues, and 5 suggestions.

### ⚠️ Critical Issues

- **SQL Injection Risk**: Direct string concatenation in query
  - **Location**: [user_service.rs](file:///home/hula/synapse_rust/src/services/user_service.rs#L45)
  - **Severity**: Critical
  - **Recommendation**: Use parameterized queries
  ```rust
  // Current (unsafe)
  let query = format!("SELECT * FROM users WHERE id = '{}'", user_id);

  // Recommended
  let query = sqlx::query!("SELECT * FROM users WHERE id = $1", user_id);
  ```

### 💡 Suggestions

- **Function Too Long**: `process_order` is 85 lines
  - **Location**: [order_service.rs](file:///home/hula/synapse_rust/src/services/order_service.rs#L12)
  - **Recommendation**: Break into smaller functions following single responsibility principle
  ```rust
  // Split into:
  // - validate_order()
  // - calculate_total()
  // - process_payment()
  // - update_inventory()
  ```

### ✅ Positive Aspects

- Excellent use of Newtype pattern for type safety
- Comprehensive error handling with custom error types
- Good documentation comments on public APIs
```

## Review Process

1. **Identify Files**: Determine which files to review
2. **Read Files**: Use Read tool to examine code
3. **Apply Checklist**: Go through each checklist item
4. **Document Findings**: Record issues with specific locations
5. **Provide Recommendations**: Offer concrete solutions with code examples
6. **Highlight Positives**: Acknowledge good practices

## Special Considerations for This Project

- **Matrix Protocol**: Ensure compliance with Matrix specification
- **E2EE**: Verify proper encryption key handling
- **Async/Await**: Check for proper async/await usage with Tokio
- **Database**: Verify SQLx query patterns and connection pooling
- **Web Framework**: Ensure Axum handler patterns are correct
