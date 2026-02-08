#!/usr/bin/env python3
import time
import requests
import subprocess
import logging
import re

# Configuration
METRICS_URL = "http://localhost:9090/metrics"
CHECK_INTERVAL = 15  # seconds
SCALE_UP_THRESHOLD = 1000  # tasks
SCALE_DOWN_THRESHOLD = 50   # tasks
MAX_WORKERS = 5
MIN_WORKERS = 1

# Setup logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger("AutoScaler")

current_workers = []

def get_queue_length():
    try:
        response = requests.get(METRICS_URL, timeout=5)
        response.raise_for_status()
        
        # Parse Prometheus format
        # synapse_worker_queue_length 42
        for line in response.text.splitlines():
            if line.startswith("synapse_worker_queue_length"):
                parts = line.split()
                if len(parts) >= 2:
                    return int(parts[1])
        return 0
    except Exception as e:
        logger.error(f"Failed to fetch metrics: {e}")
        return None

def scale_up():
    if len(current_workers) >= MAX_WORKERS:
        logger.warning(f"Max workers ({MAX_WORKERS}) reached. Cannot scale up.")
        return

    logger.info("Scaling UP: Starting new worker...")
    # In a real environment, this would call Kubernetes API or Docker CLI
    # Here we spawn a local process
    try:
        # We assume the binary is already built
        process = subprocess.Popen(
            ["cargo", "run", "--bin", "synapse_worker"],
            stdout=subprocess.DEVNULL, # Redirect logs to avoid clutter
            stderr=subprocess.DEVNULL
        )
        current_workers.append(process)
        logger.info(f"Scaled UP. Total workers: {len(current_workers) + 1} (1 initial + {len(current_workers)} dynamic)")
    except Exception as e:
        logger.error(f"Failed to start worker: {e}")

def scale_down():
    if not current_workers:
        logger.info("No dynamic workers to stop. (Keeping 1 initial worker)")
        return

    logger.info("Scaling DOWN: Stopping a worker...")
    try:
        process = current_workers.pop()
        process.terminate()
        logger.info(f"Scaled DOWN. Total workers: {len(current_workers) + 1}")
    except Exception as e:
        logger.error(f"Failed to stop worker: {e}")

def main():
    logger.info("Starting Auto-Scaler...")
    logger.info(f"Monitoring {METRICS_URL}")
    
    while True:
        queue_len = get_queue_length()
        
        if queue_len is not None:
            logger.info(f"Current Queue Length: {queue_len}")
            
            if queue_len > SCALE_UP_THRESHOLD:
                logger.warning(f"Threshold exceeded ({queue_len} > {SCALE_UP_THRESHOLD}). Triggering scale up.")
                scale_up()
            elif queue_len < SCALE_DOWN_THRESHOLD:
                if current_workers:
                    logger.info(f"Load is low ({queue_len} < {SCALE_DOWN_THRESHOLD}). Triggering scale down.")
                    scale_down()
        
        time.sleep(CHECK_INTERVAL)

if __name__ == "__main__":
    main()
