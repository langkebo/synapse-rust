#!/bin/bash
cd /Users/ljf/Desktop/hu/synapse-rust/docker/deploy

HASH=$(python3 -c "
import argon2
password = 'Admin@123'
ph = argon2.PasswordHasher(time_cost=3, memory_cost=65536, parallelism=4, hash_len=32, salt_len=16)
print(ph.hash(password))
")

docker exec synapse-postgres psql -U postgres -d synapse -c "UPDATE users SET password_hash = '$HASH' WHERE user_id = '@admin:localhost';"
echo "Admin password updated"
