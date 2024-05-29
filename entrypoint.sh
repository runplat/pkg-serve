#!/bin/bash

# http://redsymbol.net/articles/unofficial-bash-strict-mode/
set -euo pipefail
IFS=$'\n\t'

# Adapted from: https://raw.githubusercontent.com/alexeldeib/azbench/main/images/nsenter/entrypoint.sh
if [[ -z "${1}" ]]; then
    echo "Must provide a non-empty action as first argument"
    exit 1
fi

ACTION_FILE="/opt/actions/${1}"

if [[ ! -f "$ACTION_FILE" ]]; then
    echo "Expected to find action file '$ACTION_FILE', but did not exist"
    exit 1
fi

if [[ -z "${2}" ]]; then
    echo "Must provide a storage account name as the second argument"
    exit 1
fi

echo "Starting package server"
AZURE_STORAGE_ACCOUNT="${2}" ./pkg-serve &
SERVER_PID=$!

echo "Cleaning up stale actions"

rm -rf /mnt/actions/*

echo "Copying fresh actions"

cp -R /opt/actions/. /mnt/actions

while [[ ! -f /.pkg-serve/run ]]; do
    echo "Waiting for pkg-serve/run"
    sleep 1s
done

port="$(cat .pkg-serve/run)"

echo "Executing nsenter, pkg-sever listening on $port"
PKG_SERVE_RUN_PORT="${port}" nsenter -t 1 -m -- bash "${ACTION_FILE}"
RESULT="${PIPESTATUS[0]}"

kill -s SIGINT "$SERVER_PID"

if [[ "$RESULT" -eq 0 ]]; then
    # Success.
    rm -rf /mnt/actions/*
    echo "Completed successfully!"
    exit 0
else
    echo "Failed during nsenter command execution"
    exit 1
fi
