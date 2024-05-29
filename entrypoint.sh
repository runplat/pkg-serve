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

echo "Cleaning up stale actions"

rm -rf /mnt/actions/*

echo "Copying fresh actions"

cp -R /opt/actions/. /mnt/actions

echo "Starting package server"
./pkg-serve &

loops=0
while ! test -f ".pkg-serve/run"; do
    echo "Waiting for pkg-serve/run"
    sleep 10
    ((loops++))
    if [[ loops -gt 60 ]]; then
        echo 'Exceeded loop limit'
        exit 1
    fi
done

port=$(cat .pkg-serve/run)

echo "Executing nsenter, pkg-sever listening on $port"
nsenter -t 1 -m -- STORAGE_ACCOUNT_NAME="${STORAGE_ACCOUNT_NAME}" PKG_SERVE_RUN_PORT="${port}" "${ACTION_FILE}"
RESULT="${PIPESTATUS[0]}"

if [ "$RESULT" -eq 0 ]; then
    # Success.
    rm -rf /mnt/actions/*
    echo "Completed successfully!"
    sleep infinity
else
    echo "Failed during nsenter command execution"
    exit 1
fi
