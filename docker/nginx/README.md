# Nginx Configuration Guide

## Environment Variables

The nginx configuration uses environment variables for flexibility:

- `DOMAIN_NAME`: Your domain name (default: `localhost`)
- `SYNAPSE_UPSTREAM`: Backend synapse service (default: `synapse-rust:28008`)

## Usage

### Docker Compose

Add environment variables to your nginx service:

```yaml
nginx:
  image: nginx:alpine
  environment:
    - DOMAIN_NAME=${DOMAIN_NAME:-localhost}
    - SYNAPSE_UPSTREAM=synapse-rust:28008
  volumes:
    - ./docker/nginx/nginx.conf.template:/etc/nginx/nginx.conf.template:ro
    - ./docker/nginx/docker-entrypoint.sh:/docker-entrypoint.sh:ro
  entrypoint: ["/docker-entrypoint.sh"]
```

### Manual Configuration

1. Copy the template:
   ```bash
   cp docker/nginx/nginx.conf.template /etc/nginx/nginx.conf.template
   ```

2. Set environment variables:
   ```bash
   export DOMAIN_NAME=example.com
   export SYNAPSE_UPSTREAM=synapse-rust:28008
   ```

3. Generate configuration:
   ```bash
   envsubst '${DOMAIN_NAME} ${SYNAPSE_UPSTREAM}' < /etc/nginx/nginx.conf.template > /etc/nginx/nginx.conf
   ```

4. Reload nginx:
   ```bash
   nginx -s reload
   ```

## Migration from Hardcoded Configuration

If you're migrating from the old hardcoded `nginx.conf`:

1. Backup your current configuration
2. Update your docker-compose.yml to use the template
3. Set `DOMAIN_NAME` environment variable to your domain
4. Restart nginx service

## Testing

Test the configuration before applying:

```bash
DOMAIN_NAME=test.example.com envsubst '${DOMAIN_NAME} ${SYNAPSE_UPSTREAM}' < nginx.conf.template | nginx -t -c /dev/stdin
```
