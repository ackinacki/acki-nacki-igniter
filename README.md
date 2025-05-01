# Acki Nacki Igniter

- [Acki Nacki Igniter](#acki-nacki-igniter)
  - [Overview](#overview)
  - [Prerequisites](#prerequisites)
  - [Generate a Node Provider Key Pair](#generate-a-node-provider-key-pair)
  - [Get delegation signatures](#get-delegation-signatures)
  - [Prepare configuration files](#prepare-configuration-files)
    - [Generate Node Owner and BLS  Keys and Create `keys.yaml`](#generate-node-owner-and-bls--keys-and-create-keysyaml)
    - [Prepare Confirmation Signatures and Create `config.yaml`](#prepare-confirmation-signatures-and-create-configyaml)
  - [Automatic Update](#automatic-update)
  - [Run Igniter](#run-igniter)

## Overview

The Decentralized Network Starter Protocol (DNSP) collects information about nodes and licenses, optionally updates node software, and enables Zerostate (the initial state of the blockchain) generation once all DNSP requirements are satisfied.

This repository contains the source code for Acki-Nacki-Igniter, a DNSP client that runs on each of your Block Keeper (BK) servers and shares your node data with the DNSP.

Follow the instructions below to configure and run your DNSP client.

This code was tested on Ubuntu 20.04.

## Prerequisites

- Docker 
- [tvm-cli](https://dev.ackinacki.com/how-to-deploy-a-multisig-wallet#create-a-wallet-1) 

## Generate a Node Provider Key Pair

Node Provider Key Pair - a single key pair that represents an entity (a person or a company) that owns and maintains of a number of Nodes. 
All the Licenses in Acki Nacki must be delegated to BK Nodes owned by Node Provider.
License Owner signs a delegation request to a Node Provider, which, in turn, signs a delegation confirmation for the BK Node.
Even if you plan to delegate your licenses to your own nodes, you still need to generate Node Provider keys and signatures - for security purpose.

So, the first thing needed to be done is to generate Node Provider keys.  
Run:

```
tvm-cli getkeypair -o node_provider_keys.json

Seed phrase: "city young own hawk print edit service spot always limit secret suit"
Keypair successfully saved to node_provider_keys.json.
```

**Important**:  
**Store the seed phrase and secret key in a safe place. Do not share them with anyone.**

## Get Delegation Signatures

For a license to be delegated to this Node, you must obtain a delegation signature from the License Owner for each license.
A Node can operate with a minimum of one license and a maximum of ten licenses delegated to a single Node.

Provide License Owner with your Node Provider pubkey and ask them to sign a delegation.
Delegation Signature can be generated via [Acki Nacki Dashboard](hhttps://dashboard.ackinacki.com/licenses) or [manually](./docs/Manual_license_delegation.md)

## Prepare Configuration Files

Once the delegation is signed by the License Owner, do the last preparation steps:

### Generate Node Owner and BLS  Keys and Create `keys.yaml`

Generate both the [**BK Node Owner keys**](https://docs.ackinacki.com/glossary#bk-node-owner-keys) and [**BLS keys**](https://docs.ackinacki.com/glossary#bls-keys) by [following the instructions](docs/Keys_generation.md), then save them to a file named `keys.yaml.`

### Prepare Confirmation Signatures and Create `config.yaml`

Refer to the [License Delegation and Attachment](docs/License_attachment.md) section to learn how to generate the required values for the `signatures` section.

Create a [`config.yaml`](./config-template.yaml) file based on the provided template

## Automatic Update

Igniter supports automatic update of its container when a new version is released.  
To enable this feature, you need to set the `auto_update` parameter in the [`config.yaml`](./config-template.yaml#L16) configuration file:

**Important:**  
**For auto-update to work, Igniter must be launched using the `latest` tag.**

```
auto_update: true
```

## Run Igniter

**Important:**  
**Igniter must be launched on each node that you want to include in the Zerostate.**

Igniter must be run using a Docker image with the `latest` tag:

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

By default, the Gossip state is accessible at:  
http://your_public_ip_address:10001
