# caseta_listener

## What is this?

This is a project that listens to messages broadcast from a Lutron Caseta PRO hub and translates them to smart home API calls (e.g. Philips Hue API calls). This lets people interact with a smart home without needing to connect their phone to it.

_I just want guests to have as much fun with my philips hue lights as I do._

## How do you run it?

### _step zero: DHCP reservations/dns_ 
Make sure your router doesn't move things around on you. add DHCP reservations for your Caseta PRO hub, your Philips Hue hub, and any other smart devices you plan to control. If you have a dns server running, consider adding DNS entries like `philipshue.run` to make future configuration easier.

### _step one: build scene configuration files_
[smart_light_finder](https://github.com/dkulla01/smart_light_finder) has a collection of scripts to sniff out the scenes you have configured in your home. Follow the instructions there and save the `build_home_configuration` output.

### _step two: add configuration_
You need several configuration files to run this project, and you'll need environment files specifying their locations:

- `CASETA_LISTENER_REMOTE_CONFIG_FILE`: a file listing remotes with their ID, name, and type
- `CASETA_LISTENER_SCENE_CONFIG_FILE`: the output of the `build_home_configuration` script from earlier
- `CASETA_LISTENER_AUTH_CONFIGURATION_FILE`: a file with usernames/passwords/keys.
- `CASETA_LISTENER_NON_SENSITIVE_CONFIGURATION_FILE`: a file with hosts, ports, and other non-sensitive configs.

[The docker compose file](docker/docker_compose_aarch64.yaml) gives an overview of the environment vars, secrets, and configuration files that this project expects to see. Many of the specific configurations can also be overridden with environment variables.

Consider setting the `RUST_LOG` env variable to `DEBUG` or `TRACE` while developing or debugging to see more verbose log output.

### _step three: build and run_

Build the project with `cargo build` and run it with `cargo run`. Then push some buttons on your caseta remotes and see what happens.


### _step four: build docker images and run_
This project was designed to run in docker on a raspberry pi. We need a few things to make that happen:

- Build an artifact to run on aarch64 linux. You need to use the (fantastic!) [cross-rs/cross project](https://github.com/cross-rs/cross) to cross compile to that target. Install cross and then run
  ```commandline
  cross build --target aarch64-unknown-linux-gnu --release
  ```
  to build the appropriate binary.
- build the docker image with
  ```commandline
  docker build --platform linux/arm64/v8 -t caseta_listener_aarch64 -f docker/Dockerfile.aarch64 .
  ```
- get the docker image onto the target machine. I did it by saving the docker image to a `tar`, copying it to the target machine, and loading it.
  - `docker save caseta_listener_aarch64 > caseta_listener_aarch64.tar` builds the tar
  - `scp caseta_listener_aarch64.tar user@host:your/desired/path` copies the tar to the target machine
  - `docker load --input path/to/caseta_listener_aarch64.tar` loads the image from the `tar` file.
  - ensure all of the configuration files specified in the [docker_compose_aarch64.yaml](docker/docker_compose_aarch64.yaml) are in place.
  - `docker service create -d caseta_listener` to create the service.
