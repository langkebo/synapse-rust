# Stage 1: Build modules
FROM nginx:1.27.4-alpine AS builder

RUN apk update && apk add --no-cache \
    gcc \
    libc-dev \
    make \
    openssl-dev \
    pcre-dev \
    zlib-dev \
    linux-headers \
    libxml2-dev \
    libxslt-dev \
    gd-dev \
    geoip-dev \
    perl-dev \
    libedit-dev \
    bash \
    alpine-sdk \
    libmaxminddb-dev \
    wget \
    tar

# Download Nginx source and modules
RUN NGINX_VERSION=$(nginx -v 2>&1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+') && \
    wget "http://nginx.org/download/nginx-${NGINX_VERSION}.tar.gz" -O nginx.tar.gz && \
    wget "https://github.com/openresty/headers-more-nginx-module/archive/v0.37.tar.gz" -O headers-more.tar.gz && \
    wget "https://github.com/leev/ngx_http_geoip2_module/archive/3.4.tar.gz" -O geoip2.tar.gz && \
    mkdir -p nginx-src && tar -zxf nginx.tar.gz -C nginx-src --strip-components=1 && \
    tar -zxvf headers-more.tar.gz && \
    tar -zxvf geoip2.tar.gz

# Build modules
RUN cd nginx-src && \
    ./configure --with-compat \
    --add-dynamic-module=../headers-more-nginx-module-0.37 \
    --add-dynamic-module=../ngx_http_geoip2_module-3.4 && \
    make modules

# Stage 2: Final image
FROM nginx:1.27.4-alpine

# Install runtime dependencies for geoip2
RUN apk add --no-cache libmaxminddb

# Copy modules
COPY --from=builder /nginx-src/objs/ngx_http_headers_more_filter_module.so /etc/nginx/modules/
COPY --from=builder /nginx-src/objs/ngx_http_geoip2_module.so /etc/nginx/modules/

# Copy config
COPY nginx.conf /etc/nginx/nginx.conf

# Enable modules in nginx.conf
RUN sed -i '1i load_module modules/ngx_http_headers_more_filter_module.so;' /etc/nginx/nginx.conf && \
    sed -i '1i load_module modules/ngx_http_geoip2_module.so;' /etc/nginx/nginx.conf

EXPOSE 80 443
