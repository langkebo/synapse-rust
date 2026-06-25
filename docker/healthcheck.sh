#!/bin/bash

# Docker healthcheck — uses /health as the authoritative probe.
#
# /health performs a lightweight database SELECT 1 and returns:
#   - 200 {"status":"ok"}      when the DB is reachable
#   - 503 {"status":"unhealthy"} when the DB is unreachable
#
# The previous fallback to /_matrix/client/versions was removed because that
# endpoint does not touch the database, so it would mask DB outages and keep
# the container marked healthy even when the server could not serve real
# traffic. We now only fall back to a basic liveness probe if /health itself
# returns an unexpected HTTP status (e.g. 404/500), which would indicate a
# handler-level bug rather than a dependency failure.

set -e

http_code=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:8008/health 2>/dev/null || echo "000")

case "$http_code" in
    200)
        exit 0
        ;;
    503)
        echo "Healthcheck failed: database unhealthy (HTTP 503)"
        exit 1
        ;;
    *)
        # Unexpected status from /health — fall back to a basic liveness
        # check so we don't mark the container unhealthy due to a handler bug.
        if curl -sf http://localhost:8008/_matrix/client/versions >/dev/null 2>&1; then
            exit 0
        fi
        echo "Healthcheck failed: /health returned HTTP $http_code and fallback also failed"
        exit 1
        ;;
esac
