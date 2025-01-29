# Acki Nacki Igniter

The decentralized network starter protocol (DNSP), it collects Node and License information, tests and updates the node software and initiates Zerostate (first block) generation once all DNSP requirements are met.

- [Acki Nacki Igniter](#acki-nacki-igniter)
  - [Prerequisites](#prerequisites)
  - [Configuration files](#configuration-files)
  - [Run Igniter](#run-igniter)

## Prerequisites
- Docker installed
- Block Keeper wallet and BLS keys generated, see guide here: https://docs.ackinacki.com/~/changes/39mYk5N6LBvnOcXTENP9/protocol-participation/block-keeper/join-gossip-protocol?r=RLdpGPwLPtdrgnvdb2Aa 

This code was tested on Ubuntu 20.04

## Configuration files
 - Create keys file from template [keys.yaml](./keys-template.yaml)
 - Create configuration file from template: [config.yaml](./config-template.yaml) 

## Run Igniter

```
export KEYS=./keys.yaml
export CONFIG_FILE=./config.yaml

# Change the following line to match the `advertise_addr` port, specified in your config file
export ADVERTISE_PORT=10000

export IMAGE=teamgosh/acki-nacki-igniter:latest

docker run  \
        --rm \
        -p 10001:10001 \
        -p ${ADVERTISE_PORT}:10000/udp \
        -p ${ADVERTISE_PORT}:10000/tcp \
        -v "${KEYS}:/keys.yaml" \
        -v "${CONFIG_FILE}:/config.yaml" \
        -v "/var/run/docker.sock:/var/run/docker.sock" \
        $IMAGE \
        acki-nacki-igniter --keys /keys.yaml --config /config.yaml
```

By default the Gossip state is accessible on http://your_public_ip_address:10001
