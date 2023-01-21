#!/bin/sh

#run the script to stuff docker secrets into auth_config
./transform_secrets_to_yaml.sh

#execute the CMD from the dockerfile
exec "$@"
