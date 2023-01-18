#!/usr/bin/env sh


for filename in /run/secrets/*; do
  echo "$(basename "$filename"): $(cat filename)" >> "$CASETA_LISTENER_AUTH_CONFIGURATION_FILE"
done
