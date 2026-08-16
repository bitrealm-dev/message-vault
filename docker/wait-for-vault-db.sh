#!/bin/sh
# Start sqlite-web after the vault process writes data/vault.ready.
set -eu
ready=/data/vault.ready
db=/data/vault.db

while [ ! -f "$ready" ]; do
  echo "sqlite-web: waiting for $ready"
  sleep 2
done

exec sqlite_web "$db"
