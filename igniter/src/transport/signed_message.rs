use chitchat::Deserializable;
use chitchat::Serializable;
use ed25519_dalek::PUBLIC_KEY_LENGTH;
use ed25519_dalek::SIGNATURE_LENGTH;

pub const PROTOCOL_VERSION: u8 = 0;

#[derive(Debug, PartialEq)]
pub struct SignedMessage<T> {
    pub protocol_version: u8,
    pub signature: ed25519_dalek::Signature,
    pub pubkey: ed25519_dalek::VerifyingKey,
    pub message: T,
}

impl<T> SignedMessage<T> {
    pub fn new(
        protocol_version: u8,
        signature: ed25519_dalek::Signature,
        pubkey: ed25519_dalek::VerifyingKey,
        message: T,
    ) -> Self {
        Self { protocol_version, message, signature, pubkey }
    }
}

impl<T> Serializable for SignedMessage<T>
where
    T: Serializable,
{
    fn serialize(&self, buf: &mut Vec<u8>) {
        self.protocol_version.serialize(buf);

        buf.extend(self.signature.to_bytes());
        buf.extend(self.pubkey.as_bytes());

        self.message.serialize(buf);
    }

    fn serialized_len(&self) -> usize {
        self.protocol_version.serialized_len()
            + SIGNATURE_LENGTH
            + PUBLIC_KEY_LENGTH
            + self.message.serialized_len()
    }
}

impl<T> Deserializable for SignedMessage<T>
where
    T: Deserializable,
{
    fn deserialize(buf: &mut &[u8]) -> anyhow::Result<Self> {
        let protocol_version = u8::deserialize(buf)?;

        let Some((signature_buf, buf)) = buf.split_first_chunk() else {
            anyhow::bail!("failed to deserialize signature");
        };
        let signature = ed25519_dalek::Signature::from_bytes(signature_buf);

        let Some((pubkey_buf, mut buf)) = buf.split_first_chunk() else {
            anyhow::bail!("failed to deserialize pubkey");
        };
        let pubkey = ed25519_dalek::VerifyingKey::from_bytes(pubkey_buf)?;

        let message = T::deserialize(&mut buf)?;
        Ok(Self { protocol_version, signature, pubkey, message })
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::ed25519::signature::SignerMut;
    use rand::rngs::OsRng;

    use super::*;

    #[test]
    fn test_serialize_deserialize() {
        let message = "hello".to_string();
        let mut signer = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let verifying_key = signer.verifying_key();

        let signed_message = SignedMessage::new(
            PROTOCOL_VERSION,
            signer.sign(&message.as_bytes()),
            verifying_key,
            message,
        );

        let mut serialized = Vec::new();
        signed_message.serialize(&mut serialized);
        let signed_message_deser =
            SignedMessage::deserialize(&mut &serialized[..]).expect("deser failed");

        assert_eq!(signed_message, signed_message_deser);
    }
}
