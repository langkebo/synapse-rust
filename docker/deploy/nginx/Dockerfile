FROM nginx:alpine

# 移除默认 server 配置（默认监听 80，与本用途无关）
RUN rm -f /etc/nginx/conf.d/default.conf

# 覆盖默认配置（包含 rate limiting zones）
COPY nginx.conf /etc/nginx/nginx.conf
COPY prometheus.conf /etc/nginx/conf.d/prometheus.conf
COPY certs /etc/nginx/certs

EXPOSE 8081 443

CMD ["nginx", "-g", "daemon off;"]
