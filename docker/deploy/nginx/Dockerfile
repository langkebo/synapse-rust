FROM nginx:alpine

COPY prometheus.conf /etc/nginx/conf.d/prometheus.conf

EXPOSE 8081

CMD ["nginx", "-g", "daemon off;"]
