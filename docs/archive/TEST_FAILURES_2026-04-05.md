# Integration Test Failures - 2026-04-05

> Date: 2026-04-05  
> Test Run: Integration tests (`cargo test --test integration --locked`)  
> Result: 284 passed; 11 failed; 0 ignored  
> Duration: 417.91s

---

## Summary

Integration tests show 11 failures across admin, appservice, presence, and E2EE functionality. Most failures are 500 Internal Server Errors where 200/201 responses were expected.

**Pass Rate**: 96.3% (284/295 tests)

---

## Failed Tests by Category

### 1. Admin Room Management (2 failures)

#### `api_admin_room_lifecycle_tests::test_admin_room_history_purge`
- **Location**: `tests/integration/api_admin_room_lifecycle_tests.rs:273`
- **Error**: History purge should succeed (panic)
- **Expected**: Successful purge operation
- **Actual**: Operation failed
- **Priority**: P1 - Admin functionality

#### `api_admin_room_lifecycle_tests::test_admin_room_list_and_search`
- **Location**: `tests/integration/api_admin_room_lifecycle_tests.rs:377`
- **Error**: Status code mismatch
- **Expected**: 200 OK
- **Actual**: 405 Method Not Allowed
- **Priority**: P1 - Admin functionality

---

### 2. Admin User Management (2 failures)

#### `api_admin_user_lifecycle_tests::test_admin_user_lifecycle_management`
- **Location**: `tests/integration/api_admin_user_lifecycle_tests.rs:62`
- **Error**: User ID format mismatch
- **Expected**: `@testuser_1664869075:localhost`
- **Actual**: `testuser_1664869075`
- **Issue**: Missing Matrix user ID prefix/suffix formatting
- **Priority**: P1 - Data format issue

#### `api_admin_user_lifecycle_tests::test_admin_user_list_pagination_and_limits`
- **Location**: `tests/integration/api_admin_user_lifecycle_tests.rs:267`
- **Error**: Should have next_token for pagination
- **Issue**: Pagination token not returned
- **Priority**: P2 - Pagination feature

---

### 3. Application Service (4 failures)

#### `api_appservice_p1_tests::test_appservice_namespace_exclusivity`
- **Location**: `tests/integration/api_appservice_p1_tests.rs:269`
- **Error**: Status code mismatch
- **Expected**: 201 Created
- **Actual**: 500 Internal Server Error
- **Issue**: AppService creation failing
- **Priority**: P1 - Core appservice functionality

#### `api_appservice_p1_tests::test_appservice_namespace_query`
- **Location**: `tests/integration/api_appservice_p1_tests.rs:390`
- **Error**: Status code mismatch
- **Expected**: 201 Created
- **Actual**: 500 Internal Server Error
- **Priority**: P1 - Core appservice functionality

#### `api_appservice_p1_tests::test_appservice_transaction_push`
- **Location**: `tests/integration/api_appservice_p1_tests.rs:61`
- **Error**: Event push should return 201 CREATED
- **Expected**: 201 Created
- **Actual**: 500 Internal Server Error
- **Priority**: P1 - Core appservice functionality

#### `api_appservice_tests::test_appservice_virtual_user`
- **Location**: `tests/integration/api_appservice_tests.rs:137`
- **Error**: Status code mismatch
- **Expected**: 201 Created
- **Actual**: 500 Internal Server Error
- **Priority**: P1 - Virtual user creation

---

### 4. Device Presence (1 failure)

#### `api_device_presence_tests::test_presence_list_after_session_invalidation_and_relogin`
- **Location**: `tests/integration/api_device_presence_tests.rs:120`
- **Error**: Status code mismatch
- **Expected**: 200 OK
- **Actual**: 500 Internal Server Error
- **Priority**: P2 - Presence after session invalidation

---

### 5. E2EE Advanced Features (2 failures)

#### `api_e2ee_advanced_tests::test_e2ee_cross_signing_flow`
- **Location**: `tests/integration/api_e2ee_advanced_tests.rs:368`
- **Error**: Upload cross-signing keys should return 200 OK
- **Expected**: 200 OK
- **Actual**: 500 Internal Server Error
- **Priority**: P1 - Cross-signing functionality

#### `api_e2ee_advanced_tests::test_e2ee_key_backup_lifecycle`
- **Location**: `tests/integration/api_e2ee_advanced_tests.rs:141:38`
- **Error**: `called Option::unwrap() on a None value`
- **Issue**: Unexpected None value in key backup flow
- **Priority**: P1 - Key backup functionality

---

## Failure Patterns

### Pattern 1: 500 Internal Server Errors (9 tests)
Most failures are returning 500 status codes instead of expected 200/201. This suggests:
- Database errors
- Service initialization issues
- Missing dependencies or configuration
- Unhandled exceptions in request handlers

### Pattern 2: Data Format Issues (1 test)
- User ID formatting not including Matrix ID format (`@user:domain`)

### Pattern 3: Unwrap Panics (1 test)
- E2EE key backup test has an unwrap() on None value

---

## Root Cause Analysis

### Likely Causes

1. **AppService Storage/Service Issues**
   - 4 appservice tests all failing with 500 errors
   - Suggests appservice storage or service layer has a critical bug
   - May be related to recent code changes

2. **Admin API Issues**
   - Room history purge and room list endpoints failing
   - User lifecycle management has formatting bug
   - Pagination not working correctly

3. **E2EE Service Issues**
   - Cross-signing upload failing
   - Key backup returning None unexpectedly
   - May be related to recent E2EE changes

4. **Presence Service Issues**
   - Session invalidation flow not working correctly

---

## Recommended Actions

### Immediate (P0)

1. **Check server logs** for the 500 errors to identify root causes
2. **Review recent commits** that may have introduced regressions
3. **Run tests individually** with `RUST_BACKTRACE=1` to get full stack traces

### High Priority (P1)

1. **Fix AppService functionality** (4 tests)
   - Review `src/services/application_service.rs`
   - Check appservice storage layer
   - Verify appservice registration flow

2. **Fix E2EE cross-signing** (1 test)
   - Review cross-signing upload handler
   - Check key storage operations

3. **Fix admin user ID formatting** (1 test)
   - Ensure Matrix ID format is applied consistently

4. **Fix E2EE key backup unwrap** (1 test)
   - Add proper error handling instead of unwrap()
   - Return appropriate error response

### Medium Priority (P2)

5. **Fix admin room operations** (2 tests)
   - Room history purge
   - Room list/search endpoint

6. **Fix presence after session invalidation** (1 test)

7. **Fix pagination** (1 test)
   - Ensure next_token is returned when needed

---

## Testing Strategy

### Before Fixes
```bash
# Run individual failing tests with backtrace
RUST_BACKTRACE=1 cargo test --test integration test_appservice_namespace_exclusivity -- --exact --nocapture

# Check server logs
tail -f logs/synapse.log
```

### After Fixes
```bash
# Run all integration tests
cargo test --test integration --locked -- --test-threads=1

# Run specific category
cargo test --test integration api_appservice -- --test-threads=1
```

---

## Impact Assessment

### User-Facing Impact
- **High**: AppService functionality completely broken (4 tests)
- **High**: E2EE cross-signing not working (1 test)
- **Medium**: Admin operations partially broken (4 tests)
- **Low**: Presence edge case (1 test)
- **Low**: Key backup edge case (1 test)

### Development Impact
- 96.3% pass rate is acceptable for development
- Critical paths (basic messaging, rooms, sync) are passing
- Advanced features need attention

---

## Next Steps

1. Run failing tests individually with full backtraces
2. Check database state and migrations
3. Review recent commits for regressions
4. Fix AppService issues first (highest failure count)
5. Fix E2EE issues second
6. Fix admin issues third
7. Re-run full test suite to verify fixes

---

**Created**: 2026-04-05  
**Status**: Analysis Complete - Fixes Pending
