# Generate BK Node Owner and BLS Keys

To launch Igniter, you need to create a [keys.yaml](../keys-template.yaml) file based on the provided template, containing the [BK node owner keys](https://docs.ackinacki.com/glossary#bk-node-owner-keys) and the [BLS keys](https://docs.ackinacki.com/glossary#bls-keys). Instructions for generating these keys are provided below.

**Important:**  
**BK Node Owner keys and BLS keys are generated for each node.**

---
- [Generate BK Node Owner and BLS Keys](#generate-bk-node-owner-and-bls-keys)
  - [Prerequisites](#prerequisites)
  - [Generate BK Node Owner keys](#generate-bk-node-owner-keys)
  - [Generate BLS keys](#generate-bls-keys)
  - [Create a keys.yaml file](#create-a-keysyaml-file)
---

## Prerequisites

* [latest tvm-cli](https://github.com/tvmlabs/tvm-sdk/releases)
* [node-helper](https://github.com/ackinacki/ackinacki/releases)

## Generate BK Node Owner keys

Node Owner keys are used to manage a Node's wallet and perform staking operations.

Use the following command to generate a key pair from a seed phrase and save it to a file:

```
tvm-cli getkeypair -o ./bk_node_owner.keys.json
```

**Important:**  
**Write down your seed phrase and store it in a secure location.
Also, make sure the file containing the key pair is saved in a safe place.**

result:

```
Input arguments:
key_file: ./bk_node_owner.keys.json
  phrase: None
Generating seed phrase.
Seed phrase: "source artwork good relief truth reunion old review drip solid laugh found"
Keypair successfully saved to ./bk_node_owner.keys.json.
Succeeded.
```

## Generate BLS keys

Use the following command to generate BLS keys:

```
node-helper bls --path ./bk_bls.keys.json
```

As a result, the BLS keys will be saved in the file bk_bls.keys.json.  

**Note:  **
Each time you run the `node-helper bls` command, a new key pair is added to the array in the file.

Example Output:  
```
[
  {
    "public": "a03f22faaa0ae87c3676ce2018278e4e08a6423cd3043fe8ee71b1f33dff9178a8414d7a37e6d42ef5f3bce020e2d4ff",
    "secret": "05f7d62bc3c2bc60f966ce5b80b973c7e24a79f82f5b648c1186cc5f3aed9277",
    "rnd": "234291dc9e47ecb9f1265fe9e7f1ea485fe38c8daba9faf6741a836d810bc743"
  }
]
```

## Create a keys.yaml file

Create a [keys.yaml](../keys-template.yaml) file using the provided template as a reference.

