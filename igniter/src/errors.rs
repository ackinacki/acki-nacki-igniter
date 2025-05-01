use thiserror::Error;

#[derive(Error, Debug)]
pub enum IgniterError {
    #[error("\"delegation_sig\" check failed")]
    DelegationSig,

    #[error("\"delegation_confirm_sig\" check failed")]
    DelegationConfirmSig,

    #[error("\"license_proof_sig\" check failed")]
    LicenseProofSig,

    #[error("Can't deserialize data: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("No licences")]
    NoLicenses,

    #[error("Too many licences")]
    TooManyLicenses,

    #[error("Duplicate license id: {0}")]
    DuplicateLicenseId(String),

    #[error("Invalid backend public key")]
    InvalidBackedKey,

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
