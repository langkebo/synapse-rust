FROM python:3.11-alpine

# Pinned: an unpinned `pip install flask` makes the receiver image
# non-reproducible and lets an upstream major bump break alert delivery
# silently (the compose healthcheck is the only thing that would notice).
# gunicorn 是生产级 WSGI server —— app.py 里的 `app.run()` 是 Flask 开发服务器，
# 单线程、无超时/重试控制，不适合长期常驻；gunicorn 命令行启动 `app:app`。
RUN pip install --no-cache-dir flask==3.0.3 gunicorn==22.0.0

WORKDIR /app
COPY app.py /app/app.py

# 非 root 运行：本服务只读入告警并回 200，不需要任何写权限。
RUN adduser -D -u 10001 appuser \
    && chown -R appuser:appuser /app
USER appuser

EXPOSE 8080

# 单 worker + 多线程足够（webhook 接收器，负载极低）。
# --access-logfile - 把访问日志写 stdout，交给 compose 的 json-file 轮转。
CMD ["gunicorn", \
     "--bind", "0.0.0.0:8080", \
     "--workers", "1", \
     "--threads", "4", \
     "--access-logfile", "-", \
     "app:app"]
