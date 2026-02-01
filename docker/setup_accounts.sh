#!/bin/bash

BASE_URL="https://localhost"

register_user() {
    local username=$1
    local password=$2
    local res=$(curl -sk -X POST -H "Content-Type: application/json" \
        -d "{\"username\":\"$username\",\"password\":\"$password\",\"auth\":{\"type\":\"m.login.dummy\"}}" \
        "$BASE_URL/_matrix/client/r0/register")
    echo "$res"
}

login_user() {
    local username=$1
    local password=$2
    local res=$(curl -sk -X POST -H "Content-Type: application/json" \
        -d "{\"type\":\"m.login.password\",\"user\":\"$username\",\"password\":\"$password\"}" \
        "$BASE_URL/_matrix/client/r0/login")
    echo "$res"
}

echo "=== Registering Admin User ==="
admin_reg=$(register_user "admin" "admin_pass")
admin_id=$(echo $admin_reg | jq -r .user_id)
echo "Admin ID: $admin_id"

echo "=== Promoting to Admin via SQL ==="
docker exec synapse_postgres psql -U synapse -d synapse_test -c "UPDATE users SET is_admin = true WHERE user_id = '$admin_id';"

echo "=== Logging in Admin ==="
admin_login=$(login_user "admin" "admin_pass")
admin_token=$(echo $admin_login | jq -r .access_token)

echo "=== Registering Regular User 1 ==="
u1_reg=$(register_user "user1" "user1_pass")
u1_id=$(echo $u1_reg | jq -r .user_id)
u1_token=$(echo $u1_reg | jq -r .access_token)

echo "=== Registering Regular User 2 ==="
u2_reg=$(register_user "user2" "user2_pass")
u2_id=$(echo $u2_reg | jq -r .user_id)
u2_token=$(echo $u2_reg | jq -r .access_token)

echo "--- RESULTS ---"
echo "ADMIN_USER=$admin_id"
echo "ADMIN_TOKEN=$admin_token"
echo "USER1=$u1_id"
echo "USER1_TOKEN=$u1_token"
echo "USER2=$u2_id"
echo "USER2_TOKEN=$u2_token"
