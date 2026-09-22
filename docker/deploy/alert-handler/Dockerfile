FROM python:3.11-alpine

# Pinned: an unpinned `pip install flask` makes the receiver image
# non-reproducible and lets an upstream major bump break alert delivery
# silently (the compose healthcheck is the only thing that would notice).
RUN pip install --no-cache-dir flask==3.0.3

COPY app.py /app/app.py
WORKDIR /app

CMD ["python", "app.py"]
