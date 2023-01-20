#!/usr/bin/env sh


for filename in /run/secrets/*; do
  if [ -f "$filename" ]; then
    echo "$(basename "$filename"): $(cat "$filename")" >> "$CASETA_LISTENER_AUTH_CONFIGURATION_FILE"
  fi
done
