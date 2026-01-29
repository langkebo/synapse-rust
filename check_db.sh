#!/bin/bash
export PGPASSWORD="synapse_password"
psql -h localhost -U synapse_user -d synapse_db -c "\d access_tokens" 2>&1
echo "---"
psql -h localhost -U synapse_user -d synapse_db -c "\d refresh_tokens" 2>&1
