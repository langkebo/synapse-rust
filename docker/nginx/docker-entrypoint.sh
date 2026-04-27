#!/bin/sh
set -e

# Substitute environment variables in nginx config template
envsubst '${DOMAIN_NAME} ${SYNAPSE_UPSTREAM}' < /etc/nginx/nginx.conf.template > /etc/nginx/nginx.conf

# Start nginx
exec nginx -g 'daemon off;'
