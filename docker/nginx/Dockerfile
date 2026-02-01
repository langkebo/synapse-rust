# Use Alpine packages for nginx dynamic modules to avoid fetching sources during build
FROM nginx:alpine

RUN apk add --no-cache \
    libmaxminddb \
    nginx-mod-http-headers-more \
    nginx-mod-http-geoip2

# Copy config
COPY nginx.conf /etc/nginx/nginx.conf

# Enable modules in nginx.conf
RUN sed -i '1i load_module modules/ngx_http_headers_more_filter_module.so;' /etc/nginx/nginx.conf && \
    sed -i '1i load_module modules/ngx_http_geoip2_module.so;' /etc/nginx/nginx.conf

EXPOSE 80 443
