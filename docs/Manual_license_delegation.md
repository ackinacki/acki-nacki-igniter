# Manual License Delegation

- [Prerequisites](#prerequisites)
- [Obtaining the Delegation Signature](#obtaining-the-delegation-signature)

When you delegated your licenses through the Dashboard, the Delegation Signature (`delegation_sig`) is generated automatically. The Node Provider will need this signature to correctly configure and launch the Decentralized Network Starter Protocol (DNSP).

**Note:**  
If you are the owner of multiple nodes, you are also considered a Node Provider.

---
## Prerequisites

- [latest tvm-cli](https://github.com/tvmlabs/tvm-sdk/releases)


## Obtaining the Delegation Signature

To obtain the `delegation_sig`, concatenate the following values into a single line (without spaces or newlines), then encode the result as a Base64 string:

- `license_owner_pubkey`
- `provider_pubkey`
- `license_id`
- `timestamp` (current time in seconds, safe it as it will be needed to validate the signature later) 

* For `provider_pubkey`, use the public key of the company or individual to whom you are delegating the license.
If you're delegating the license it to yourself, must still generate your own [`Node Provider Key Pair`](https://github.com/ackinacki/acki-nacki-igniter/blob/main/README.md/#generate-a-node-provider-key-pair) for security purposes and use its public key.

* You can view the `license_id` of each license in the Dashboard under the **Licenses** tab:

[licence ID](https://github.com/ackinacki/acki-nacki-igniter/blob/main/docs/licence ID.jpg)

* Retrieve the current `timestamp` in seconds and save it for later use.

For example:  

```bash

timestamp=$(date +%s) 
echo $timestamp

license_id=2aebf602-7503-4572-976c-79f206f9b2c0
license_owner_pubkey=7876682d123554aeedc71eb4e437e3c25ea8c9d97c0fd3fb9521061d6f494cdc
provider_pubkey=b8727272b106cd6b0712d18a747432577256e0a14f73e5a187a2f98e175034fc


echo -n ${license_owner_pubkey}${provider_pubkey}${license_id}${timestamp} | base64 -w 0
```

**Note:**  
If you use `zsh` you can see the `%` sign at the end of your output which is not actually part of the Base64-encoded string.

Sign this **Base64 string** using tvm-cli:

```
 tvm-cli sign \
    --keys license_owner_keys.json   \
    --data MmFlYmY2MDItNzUwMy00NTcyLTk3NmMtNzlmMjA2ZjliMmMwNzg3NjY4MmQxMjM1NTRhZWVkYzcxZWI0ZTQzN2UzYzI1ZWE4YzlkOTdjMGZkM2ZiOTUyMTA2MWQ2ZjQ5NGNkY2I4NzI3MjcyYjEwNmNkNmIwNzEyZDE4YTc0NzQzMjU3NzI1NmUwYTE0ZjczZTVhMTg3YTJmOThlMTc1MDM0ZmMxNzQ0MDMyMTg1
```

An example output:

```
Signature: FnqzFgemmk/+RFu/OQMD25v0bShR4/0k8oiovwcsr1tSmqSu8aZEwoRJ8nSOefEN8ZF2v9pvEqI8moY6pqamAQ==
```

This is your **Delegation signature** (`delegation_sig`).
Share it along with the `timestamp` with your Node Provider.
Or, if you are delegating the licenses to your own nodes, continue configuring the DNSP client.
