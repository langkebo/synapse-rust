#!/bin/bash

set -e

if curl -sf http://localhost:28008/health > /dev/null 2>&1; then
    exit 0
fi

if curl -sf http://localhost:28008/_matrix/client/versions > /dev/null 2>&1; then
    exit 0
fi

if curl -sf http://localhost:28008/_matrix/federation/v1/version > /dev/null 2>&1; then
    exit 0
fi

echo "Healthcheck failed"
exit 1
