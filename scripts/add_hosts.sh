#!/bin/bash

# Matrix Server Hosts Configuration Script
# Run this script with sudo: sudo ./add_hosts.sh

HOSTS_FILE="/etc/hosts"
BACKUP_FILE="/etc/hosts.backup.$(date +%Y%m%d_%H%M%S)"

# Create backup
echo "Creating backup of current hosts file..."
sudo cp "$HOSTS_FILE" "$BACKUP_FILE"

# Add Matrix server entries
echo "Adding Matrix server entries to /etc/hosts..."

# Check if entries already exist
if sudo grep -q "cjystx.top" "$HOSTS_FILE" > /dev/null 2>&1; then
    echo "Entries already exist in /etc/hosts, skipping..."
else
    echo "" | sudo tee -a "$HOSTS_FILE" > /dev/null
    echo "# Matrix Server - cjystx.top (added $(date))" | sudo tee -a "$HOSTS_FILE" > /dev/null
    echo "127.0.0.1 cjystx.top" | sudo tee -a "$HOSTS_FILE" > /dev/null
    echo "127.0.0.1 matrix.cjystx.top" | sudo tee -a "$HOSTS_FILE" > /dev/null
    echo "Done! Entries added to /etc/hosts"
fi

echo ""
echo "Current /etc/hosts content related to Matrix:"
sudo grep -E "cjystx|matrix" "$HOSTS_FILE" || echo "No Matrix entries found"

echo ""
echo "========================================"
echo "To apply changes, you may need to flush DNS cache:"
echo "Run: sudo dscacheutil -flushcache"
echo "========================================"
