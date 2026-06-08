import hashlib
import json
import os

queries = {
    "create_verification_request": {
        "query": """INSERT INTO device_verification_request
             (user_id, new_device_id, requesting_device_id, verification_method,
              status, request_token, commitment, pubkey, created_ts, expires_at, completed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, to_timestamp($10::double precision / 1000.0), to_timestamp($11::double precision / 1000.0))""",
        "describe": {
            "columns": [],
            "parameters": {
                "Left": [
                    "Text", "Text", "Text", "Text", "Text", "Text", "Text", "Text",
                    "Int8", "Float8", "Float8"
                ]
            },
            "nullable": []
        }
    },
    "get_request_by_token": {
        "query": """SELECT
                id,
                user_id,
                new_device_id,
                requesting_device_id,
                verification_method,
                status,
                request_token,
                commitment,
                pubkey,
                created_ts,
                (EXTRACT(EPOCH FROM expires_at) * 1000)::BIGINT AS "expires_at!",
                CASE WHEN completed_at IS NOT NULL THEN (EXTRACT(EPOCH FROM completed_at) * 1000)::BIGINT ELSE NULL END AS completed_at
            FROM device_verification_request
            WHERE request_token = $1""",
        "describe": {
            "columns": [
                {"ordinal": 0, "name": "id", "type_info": "Int8"},
                {"ordinal": 1, "name": "user_id", "type_info": "Text"},
                {"ordinal": 2, "name": "new_device_id", "type_info": "Text"},
                {"ordinal": 3, "name": "requesting_device_id", "type_info": "Text"},
                {"ordinal": 4, "name": "verification_method", "type_info": "Text"},
                {"ordinal": 5, "name": "status", "type_info": "Text"},
                {"ordinal": 6, "name": "request_token", "type_info": "Text"},
                {"ordinal": 7, "name": "commitment", "type_info": "Text"},
                {"ordinal": 8, "name": "pubkey", "type_info": "Text"},
                {"ordinal": 9, "name": "created_ts", "type_info": "Int8"},
                {"ordinal": 10, "name": "expires_at!", "type_info": "Int8"},
                {"ordinal": 11, "name": "completed_at", "type_info": "Int8"}
            ],
            "parameters": {"Left": ["Text"]},
            "nullable": [False, False, False, True, False, False, False, True, True, False, None, True]
        }
    },
    "get_pending_request": {
        "query": """SELECT
                id,
                user_id,
                new_device_id,
                requesting_device_id,
                verification_method,
                status,
                request_token,
                commitment,
                pubkey,
                created_ts,
                (EXTRACT(EPOCH FROM expires_at) * 1000)::BIGINT AS "expires_at!",
                CASE WHEN completed_at IS NOT NULL THEN (EXTRACT(EPOCH FROM completed_at) * 1000)::BIGINT ELSE NULL END AS completed_at
            FROM device_verification_request
            WHERE user_id = $1 AND new_device_id = $2 AND status = 'pending' AND expires_at > NOW()""",
        "describe": {
            "columns": [
                {"ordinal": 0, "name": "id", "type_info": "Int8"},
                {"ordinal": 1, "name": "user_id", "type_info": "Text"},
                {"ordinal": 2, "name": "new_device_id", "type_info": "Text"},
                {"ordinal": 3, "name": "requesting_device_id", "type_info": "Text"},
                {"ordinal": 4, "name": "verification_method", "type_info": "Text"},
                {"ordinal": 5, "name": "status", "type_info": "Text"},
                {"ordinal": 6, "name": "request_token", "type_info": "Text"},
                {"ordinal": 7, "name": "commitment", "type_info": "Text"},
                {"ordinal": 8, "name": "pubkey", "type_info": "Text"},
                {"ordinal": 9, "name": "created_ts", "type_info": "Int8"},
                {"ordinal": 10, "name": "expires_at!", "type_info": "Int8"},
                {"ordinal": 11, "name": "completed_at", "type_info": "Int8"}
            ],
            "parameters": {"Left": ["Text", "Text"]},
            "nullable": [False, False, False, True, False, False, False, True, True, False, None, True]
        }
    },
    "update_request_status": {
        "query": """UPDATE device_verification_request
             SET status = $1, completed_at = to_timestamp($2::double precision / 1000.0)
             WHERE request_token = $3""",
        "describe": {
            "columns": [],
            "parameters": {"Left": ["Text", "Float8", "Text"]},
            "nullable": []
        }
    },
    "cleanup_expired_requests": {
        "query": """UPDATE device_verification_request
             SET status = 'expired', completed_at = NOW()
             WHERE status = 'pending' AND expires_at < NOW()""",
        "describe": {
            "columns": [],
            "parameters": {"Left": []},
            "nullable": []
        }
    },
    "log_key_rotation": {
        "query": """INSERT INTO key_rotation_log
             (user_id, device_id, room_id, rotation_type, old_key_id, new_key_id, reason, rotated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8::double precision / 1000.0))""",
        "describe": {
            "columns": [],
            "parameters": {"Left": ["Text", "Text", "Text", "Text", "Text", "Text", "Text", "Float8"]},
            "nullable": []
        }
    }
}

sqlx_dir = "/Users/ljf/Desktop/hu_ts/synapse-rust/.sqlx"

for name, data in queries.items():
    query_text = data["query"]
    hash_val = hashlib.sha256(query_text.encode()).hexdigest()
    entry = {
        "db_name": "PostgreSQL",
        "query": query_text,
        "describe": data["describe"],
        "hash": hash_val
    }
    filepath = os.path.join(sqlx_dir, f"query-{hash_val}.json")
    with open(filepath, 'w') as f:
        json.dump(entry, f, indent=2)
    print(f"Created {filepath} for {name}")
