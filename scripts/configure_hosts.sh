#!/bin/bash

# Matrix Server Hosts Configuration Script
# This script adds domain resolution for cjystx.top and matrix.cjystx.top

echo "========================================"
echo "Matrix Server Hosts Configuration"
echo "========================================"
echo ""
echo "This script will add the following entries to /etc/hosts:"
echo "  127.0.0.1 cjystx.top"
echo "  127.0.0.1 matrix.cjystx.top"
echo ""

# Check if entries already exist
if grep -q "cjystx.top" /etc/hosts 2>/dev/null; then
    echo "Warning: cjystx.top entries already exist in /etc/hosts"
    echo "Current entries:"
    grep "cjystx.top" /etc/hosts
    echo ""
    read -p "Do you want to update them? (y/n): " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 0
    fi
    # Remove old entries
    sudo sed -i '' '/cjystx.top/d' /etc/hosts
fi

# Add new entries
echo "Adding entries to /etc/hosts..."
sudo bash -c 'cat >> /etc/hosts << EOF

# Matrix Server - cjystx.top
127.0.0.1 cjystx.top
127.0.0.1 matrix.cjystx.top
EOF'

# Verify
echo ""
echo "Verification - Current /etc/hosts entries for cjystx.top:"
grep "cjystx.top" /etc/hosts

echo ""
echo "Testing DNS resolution..."
ping -c 1 cjystx.top 2>/dev/null && echo "cjystx.top resolves to 127.0.0.1 ✓" || echo "cjystx.top resolution failed ✗"
ping -c 1 matrix.cjystx.top 2>/dev/null && echo "matrix.cjystx.top resolves to 127.0.0.1 ✓" || echo "matrix.cjystx.top resolution failed ✗"

echo ""
echo "========================================"
echo "Configuration Complete!"
echo "========================================"
