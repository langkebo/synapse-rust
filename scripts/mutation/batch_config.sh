#!/usr/bin/env bash
# Mutation Testing Batch Configuration
# Generated: 2026-06-12
# Total: 24 batches covering ~499 .rs files under src/
#
# Usage:
#   source scripts/mutation/batch_config.sh
#   run_batch "batch_01_cache" "${BATCH_01_CACHE[@]}"

set -eo pipefail

# ── Batch 01: Cache (6 files) ──
BATCH_01_CACHE=(
    src/cache/mod.rs
    src/cache/redis.rs
    src/cache/memory.rs
    src/cache/cache_manager.rs
    src/cache/cache_key.rs
    src/cache/serialization.rs
)

# ── Batch 02: Auth (8 files) ──
BATCH_02_AUTH=(
    src/auth/mod.rs
    src/auth/login.rs
    src/auth/password.rs
    src/auth/token.rs
    src/auth/refresh_token.rs
    src/auth/registration.rs
    src/auth/oidc.rs
    src/auth/sso.rs
)

# ── Batch 03: Worker (10 files) ──
BATCH_03_WORKER=(
    src/worker/mod.rs
    src/worker/queue.rs
    src/worker/health.rs
    src/worker/metrics.rs
    src/worker/replication.rs
    src/worker/bus.rs
    src/worker/load_balancer.rs
    src/worker/ping.rs
    src/worker/stream_writer.rs
    src/worker/config.rs
)

# ── Batch 04: Federation (16 files) ──
BATCH_04_FEDERATION=(
    src/federation/mod.rs
    src/federation/client.rs
    src/federation/server.rs
    src/federation/transaction.rs
    src/federation/authorization.rs
    src/federation/signing.rs
    src/federation/keys.rs
    src/federation/event.rs
    src/federation/query.rs
    src/federation/membership.rs
    src/federation/backfill.rs
    src/federation/state.rs
    src/federation/device.rs
    src/federation/edus.rs
    src/federation/make_join.rs
    src/federation/send_join.rs
)

# ── Batch 05: Common config (15 files) ──
BATCH_05_COMMON_CONFIG=(
    src/common/config/mod.rs
    src/common/config/server.rs
    src/common/config/database.rs
    src/common/config/redis.rs
    src/common/config/federation.rs
    src/common/config/rate_limit.rs
    src/common/config/security.rs
    src/common/config/cors.rs
    src/common/config/search.rs
    src/common/config/smtp.rs
    src/common/config/worker.rs
    src/common/config/identity.rs
    src/common/config/logging.rs
    src/common/config/telemetry.rs
    src/common/config/admin.rs
)

# ── Batch 06: Common (remaining ~46 files) ──
BATCH_06_COMMON=(src/common/*.rs)
# Expand glob
BATCH_06_COMMON_FILES=()
for f in src/common/*.rs; do
    if [[ -f "$f" ]] && [[ ! "$f" =~ config/ ]]; then
        BATCH_06_COMMON_FILES+=("$f")
    fi
done

# ── Batch 07: E2EE backup + cross_signing (~15 files) ──
BATCH_07_E2EE_BACKUP_CROSS=(
    src/e2ee/backup/mod.rs
    src/e2ee/backup/service.rs
    src/e2ee/backup/storage.rs
    src/e2ee/backup/models.rs
    src/e2ee/backup/keys.rs
    src/e2ee/backup/session.rs
    src/e2ee/backup/version.rs
    src/e2ee/cross_signing/mod.rs
    src/e2ee/cross_signing/service.rs
    src/e2ee/cross_signing/storage.rs
    src/e2ee/cross_signing/models.rs
    src/e2ee/cross_signing/keys.rs
    src/e2ee/cross_signing/signing.rs
    src/e2ee/cross_signing/bootstrap.rs
    src/e2ee/cross_signing/verification.rs
)

# ── Batch 08: E2EE device_keys + key_request (~15 files) ──
BATCH_08_E2EE_DEVICE_KEYS=(
    src/e2ee/device_keys/mod.rs
    src/e2ee/device_keys/service.rs
    src/e2ee/device_keys/storage.rs
    src/e2ee/device_keys/models.rs
    src/e2ee/device_keys/upload.rs
    src/e2ee/device_keys/query.rs
    src/e2ee/device_keys/claim.rs
    src/e2ee/device_keys/signing.rs
    src/e2ee/key_request/mod.rs
    src/e2ee/key_request/service.rs
    src/e2ee/key_request/storage.rs
    src/e2ee/key_request/models.rs
    src/e2ee/key_request/forwarding.rs
    src/e2ee/key_request/gossip.rs
    src/e2ee/key_request/verification.rs
)

# ── Batch 09: E2EE key_rotation + megolm (~15 files) ──
BATCH_09_E2EE_KEY_MEGOLM=(
    src/e2ee/key_rotation/mod.rs
    src/e2ee/key_rotation/service.rs
    src/e2ee/key_rotation/storage.rs
    src/e2ee/key_rotation/models.rs
    src/e2ee/key_rotation/scheduler.rs
    src/e2ee/key_rotation/cleanup.rs
    src/e2ee/megolm/mod.rs
    src/e2ee/megolm/provider.rs
    src/e2ee/megolm/session.rs
    src/e2ee/megolm/inbound.rs
    src/e2ee/megolm/outbound.rs
    src/e2ee/megolm/ratchet.rs
    src/e2ee/megolm/keys.rs
    src/e2ee/megolm/decrypt.rs
    src/e2ee/megolm/encrypt.rs
)

# ── Batch 10: E2EE remaining (ssss, to_device, verification, olm, mod.rs) ~12 files ──
BATCH_10_E2EE_REMAINING=(
    src/e2ee/mod.rs
    src/e2ee/ssss/mod.rs
    src/e2ee/ssss/service.rs
    src/e2ee/ssss/storage.rs
    src/e2ee/to_device/mod.rs
    src/e2ee/to_device/service.rs
    src/e2ee/to_device/storage.rs
    src/e2ee/verification/mod.rs
    src/e2ee/verification/service.rs
    src/e2ee/verification/storage.rs
    src/e2ee/olm/mod.rs
    src/e2ee/olm/account.rs
)

# ── Batch 11: Storage event (~14 files) ──
BATCH_11_STORAGE_EVENT=(
    src/storage/event/mod.rs
    src/storage/event/storage.rs
    src/storage/event/queries.rs
    src/storage/event/pagination.rs
    src/storage/event/redaction.rs
    src/storage/event/state.rs
    src/storage/event/relations.rs
    src/storage/event/search.rs
    src/storage/event/visibility.rs
    src/storage/event/aggregation.rs
    src/storage/event/backfill.rs
    src/storage/event/sync.rs
    src/storage/event/read_marker.rs
    src/storage/event/thread.rs
)

# ── Batch 12: Storage media (~8 files) ──
BATCH_12_STORAGE_MEDIA=(
    src/storage/media/mod.rs
    src/storage/media/storage.rs
    src/storage/media/queries.rs
    src/storage/media/upload.rs
    src/storage/media/download.rs
    src/storage/media/thumbnail.rs
    src/storage/media/remote.rs
    src/storage/media/cleanup.rs
)

# ── Batch 13: Storage room (~13 files) ──
BATCH_13_STORAGE_ROOM=(
    src/storage/room/mod.rs
    src/storage/room/storage.rs
    src/storage/room/queries.rs
    src/storage/room/membership.rs
    src/storage/room/state.rs
    src/storage/room/alias.rs
    src/storage/room/tags.rs
    src/storage/room/account_data.rs
    src/storage/room/create.rs
    src/storage/room/summary.rs
    src/storage/room/visibility.rs
    src/storage/room/upgrade.rs
    src/storage/room/space.rs
)

# ── Batch 14: Storage (first 15 top-level files) ──
BATCH_14_STORAGE_TOP_A=(
    src/storage/mod.rs
    src/storage/user.rs
    src/storage/device.rs
    src/storage/access_token.rs
    src/storage/refresh_token.rs
    src/storage/threepid.rs
    src/storage/presence.rs
    src/storage/qr_login.rs
    src/storage/invite_blocklist.rs
    src/storage/sticky_event.rs
    src/storage/email_verification.rs
    src/storage/key_backup.rs
    src/storage/schema_health_check.rs
    src/storage/database.rs
    src/storage/migrations.rs
)

# ── Batch 15: Storage (remaining top-level files) ──
BATCH_15_STORAGE_TOP_B=(
    src/storage/push_rule.rs
    src/storage/push.rs
    src/storage/notification.rs
    src/storage/filter.rs
    src/storage/directory.rs
    src/storage/profile.rs
    src/storage/receipt.rs
    src/storage/typing.rs
    src/storage/report.rs
    src/storage/registration_token.rs
    src/storage/password_reset.rs
    src/storage/rate_limit.rs
    src/storage/transaction_id.rs
    src/storage/event_auth.rs
    src/storage/metrics.rs
)

# ── Batch 16: Services auth + assemble (~20 files) ──
BATCH_16_SERVICES_AUTH_ASSEMBLE=(
    src/services/mod.rs
    src/services/container.rs
    src/services/auth/mod.rs
    src/services/auth/service.rs
    src/services/auth/password.rs
    src/services/auth/token.rs
    src/services/auth/oidc.rs
    src/services/assemble/mod.rs
    src/services/assemble/event.rs
    src/services/assemble/room.rs
    src/services/assemble/state.rs
    src/services/assemble/membership.rs
    src/services/assemble/profile.rs
    src/services/assemble/redaction.rs
    src/services/assemble/notification.rs
    src/services/assemble/push.rs
    src/services/assemble/typing.rs
    src/services/assemble/receipt.rs
    src/services/assemble/read_marker.rs
    src/services/assemble/filter.rs
)

# ── Batch 17: Services room + space (~20 files) ──
BATCH_17_SERVICES_ROOM=(
    src/services/room/mod.rs
    src/services/room/service.rs
    src/services/room/creation.rs
    src/services/room/join.rs
    src/services/room/leave.rs
    src/services/room/invite.rs
    src/services/room/kick.rs
    src/services/room/ban.rs
    src/services/room/state.rs
    src/services/room/alias.rs
    src/services/room/visibility.rs
    src/services/room/upgrade.rs
    src/services/room/tombstone.rs
    src/services/room/summary.rs
    src/services/room/peek.rs
    src/services/room/knock.rs
    src/services/room/space/mod.rs
    src/services/room/space/service.rs
    src/services/room/space/hierarchy.rs
    src/services/room/space/summary.rs
)

# ── Batch 18: Services sync + sliding_sync (~15 files) ──
BATCH_18_SERVICES_SYNC=(
    src/services/sync_service/mod.rs
    src/services/sync_service/service.rs
    src/services/sync_service/response.rs
    src/services/sync_service/timeline.rs
    src/services/sync_service/state.rs
    src/services/sync_service/account_data.rs
    src/services/sync_service/presence.rs
    src/services/sync_service/to_device.rs
    src/services/sync_service/device_lists.rs
    src/services/sync_service/groups.rs
    src/services/sync_service/filter.rs
    src/services/sync_service/rooms.rs
    src/services/sync_service/token.rs
    src/services/sync_service/notification.rs
    src/services/sync_service/read_marker.rs
)

# ── Batch 19: Services media + identity + rtc (~20 files) ──
BATCH_19_SERVICES_MEDIA_ID=(
    src/services/media/mod.rs
    src/services/media/service.rs
    src/services/media/upload.rs
    src/services/media/download.rs
    src/services/media/thumbnail.rs
    src/services/media/preview.rs
    src/services/media/processing.rs
    src/services/media/remote.rs
    src/services/media/storage.rs
    src/services/identity/mod.rs
    src/services/identity/service.rs
    src/services/identity/storage.rs
    src/services/identity/models.rs
    src/services/rtc/mod.rs
    src/services/rtc/service.rs
    src/services/rtc/models.rs
    src/services/rtc/domain.rs
    src/services/rtc/call.rs
    src/services/rtc/turn.rs
    src/services/rtc/voip.rs
)

# ── Batch 20: Services push + e2ee + remaining (~20 files) ──
BATCH_20_SERVICES_PUSH=(
    src/services/push/mod.rs
    src/services/push/service.rs
    src/services/push/rules.rs
    src/services/push/pushers.rs
    src/services/push/notifications.rs
    src/services/push/gateway.rs
    src/services/push/providers/mod.rs
    src/services/push/providers/apns.rs
    src/services/push/providers/fcm.rs
    src/services/push/providers/webpush.rs
    src/services/e2ee/mod.rs
    src/services/e2ee/service.rs
    src/services/geo_ip/mod.rs
    src/services/geo_ip/service.rs
    src/services/database_initializer/mod.rs
    src/services/database_initializer/service.rs
    src/services/content_scanner/mod.rs
    src/services/content_scanner/service.rs
    src/services/friend_room_service/mod.rs
    src/services/friend_room_service/service.rs
)

# ── Batch 21: Web middleware + utils (~20 files) ──
BATCH_21_WEB_MIDDLEWARE=(
    src/web/middleware/auth.rs
    src/web/middleware/cors.rs
    src/web/middleware/rate_limit.rs
    src/web/middleware/security.rs
    src/web/middleware/csrf.rs
    src/web/middleware/compression.rs
    src/web/middleware/logging.rs
    src/web/middleware/metrics.rs
    src/web/middleware/request_id.rs
    src/web/middleware/user_agent.rs
    src/web/utils/auth.rs
    src/web/utils/error.rs
    src/web/utils/response.rs
    src/web/utils/validation.rs
    src/web/utils/pagination.rs
    src/web/utils/json.rs
    src/web/utils/headers.rs
    src/web/utils/query.rs
    src/web/utils/url.rs
    src/web/utils/rate_limit.rs
)

# ── Batch 22: Web routes extractors + handlers (~20 files) ──
BATCH_22_WEB_EXTRACTORS=(
    src/web/routes/mod.rs
    src/web/routes/assembly.rs
    src/web/routes/extractors/mod.rs
    src/web/routes/extractors/auth.rs
    src/web/routes/extractors/json.rs
    src/web/routes/extractors/pagination.rs
    src/web/routes/extractors/query.rs
    src/web/routes/extractors/path.rs
    src/web/routes/extractors/client_info.rs
    src/web/routes/handlers/mod.rs
    src/web/routes/handlers/versions.rs
    src/web/routes/handlers/room/mod.rs
    src/web/routes/handlers/room/create.rs
    src/web/routes/handlers/room/join.rs
    src/web/routes/handlers/room/leave.rs
    src/web/routes/handlers/room/invite.rs
    src/web/routes/handlers/room/state.rs
    src/web/routes/handlers/room/messages.rs
    src/web/routes/handlers/room/members.rs
    src/web/routes/handlers/room/redact.rs
)

# ── Batch 23: Web routes (top-level) ~20 files ──
BATCH_23_WEB_ROUTES_A=(
    src/web/websocket.rs
    src/web/mod.rs
    src/web/api_doc.rs
    src/web/routes/account_compat.rs
    src/web/routes/auth_compat.rs
    src/web/routes/device.rs
    src/web/routes/e2ee_routes.rs
    src/web/routes/keys.rs
    src/web/routes/media.rs
    src/web/routes/presence.rs
    src/web/routes/profile.rs
    src/web/routes/push_routes.rs
    src/web/routes/room_summary.rs
    src/web/routes/search.rs
    src/web/routes/sync.rs
    src/web/routes/threepid.rs
    src/web/routes/typing.rs
    src/web/routes/voip.rs
    src/web/routes/well_known.rs
    src/web/routes/widget.rs
)

# ── Batch 24: Web routes remaining + admin + federation + space (~20 files) ──
BATCH_24_WEB_ROUTES_B=(
    src/web/routes/admin/mod.rs
    src/web/routes/admin/room/mod.rs
    src/web/routes/admin/room/delete.rs
    src/web/routes/admin/room/quarantine.rs
    src/web/routes/admin/room/list.rs
    src/web/routes/admin/room/details.rs
    src/web/routes/federation/mod.rs
    src/web/routes/federation/authorization.rs
    src/web/routes/federation/backfill.rs
    src/web/routes/federation/event.rs
    src/web/routes/federation/keys.rs
    src/web/routes/federation/membership.rs
    src/web/routes/federation/query.rs
    src/web/routes/federation/state.rs
    src/web/routes/federation/transaction.rs
    src/web/routes/space/mod.rs
    src/web/routes/space/hierarchy.rs
    src/web/routes/space/summary.rs
    src/web/routes/space/rooms.rs
    src/web/routes/space/join.rs
)

# ── Batch index ──
# Ordered list of all batch IDs
ALL_BATCH_IDS=(
    batch_01_cache
    batch_02_auth
    batch_03_worker
    batch_04_federation
    batch_05_common_config
    batch_06_common
    batch_07_e2ee_backup_cross
    batch_08_e2ee_device_keys
    batch_09_e2ee_key_megolm
    batch_10_e2ee_remaining
    batch_11_storage_event
    batch_12_storage_media
    batch_13_storage_room
    batch_14_storage_top_a
    batch_15_storage_top_b
    batch_16_services_auth_assemble
    batch_17_services_room
    batch_18_services_sync
    batch_19_services_media_id
    batch_20_services_push
    batch_21_web_middleware
    batch_22_web_extractors
    batch_23_web_routes_a
    batch_24_web_routes_b
)

# Batch description lookup function (bash 3.2 compatible)
get_batch_desc() {
    case "$1" in
        batch_01_cache)              echo "Cache layer (6 files)" ;;
        batch_02_auth)               echo "Auth module (8 files)" ;;
        batch_03_worker)             echo "Worker module (10 files)" ;;
        batch_04_federation)         echo "Federation module (16 files)" ;;
        batch_05_common_config)      echo "Common config (15 files)" ;;
        batch_06_common)             echo "Common remaining (46 files)" ;;
        batch_07_e2ee_backup_cross)  echo "E2EE backup + cross-signing (15 files)" ;;
        batch_08_e2ee_device_keys)   echo "E2EE device_keys + key_request (15 files)" ;;
        batch_09_e2ee_key_megolm)    echo "E2EE key_rotation + megolm (15 files)" ;;
        batch_10_e2ee_remaining)     echo "E2EE remaining (12 files)" ;;
        batch_11_storage_event)      echo "Storage event (14 files)" ;;
        batch_12_storage_media)      echo "Storage media (8 files)" ;;
        batch_13_storage_room)       echo "Storage room (13 files)" ;;
        batch_14_storage_top_a)      echo "Storage top-level A (15 files)" ;;
        batch_15_storage_top_b)      echo "Storage top-level B (15 files)" ;;
        batch_16_services_auth_assemble) echo "Services auth + assemble (20 files)" ;;
        batch_17_services_room)      echo "Services room + space (20 files)" ;;
        batch_18_services_sync)      echo "Services sync (15 files)" ;;
        batch_19_services_media_id)  echo "Services media + identity + rtc (20 files)" ;;
        batch_20_services_push)      echo "Services push + e2ee + remaining (20 files)" ;;
        batch_21_web_middleware)     echo "Web middleware + utils (20 files)" ;;
        batch_22_web_extractors)     echo "Web extractors + handlers (20 files)" ;;
        batch_23_web_routes_a)       echo "Web routes top-level (20 files)" ;;
        batch_24_web_routes_b)       echo "Web routes admin + federation + space (20 files)" ;;
        *)                           echo "Unknown batch: $1" ;;
    esac
}