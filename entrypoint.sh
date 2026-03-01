#!/bin/sh
set -e

# Start admin API in background
gate-admin &

# Start proxy in foreground (blocks until exit)
exec gate-proxy
