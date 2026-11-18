use core::fmt::{self, Write};

use heapless::String;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A start-response secret, never part of readable run identity or evidence.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TuningOwnerProof(String<64>);

impl fmt::Debug for TuningOwnerProof {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

impl TuningOwnerProof {
    pub fn from_random_bytes(bytes: [u8; 32]) -> Self {
        let mut value = String::new();
        for byte in bytes {
            let _ = write!(value, "{byte:02x}");
        }
        Self(value)
    }

    fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.0.as_bytes()).into()
    }
}

/// Only the verifier is retained in the firmware run, not the bearer proof.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct TuningOwnerVerifier(Option<[u8; 32]>);

impl fmt::Debug for TuningOwnerVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

impl TuningOwnerVerifier {
    pub const fn empty() -> Self {
        Self(None)
    }

    pub fn bind(&mut self, proof: &TuningOwnerProof) -> bool {
        if self.0.is_some() {
            return false;
        }
        self.0 = Some(proof.digest());
        true
    }

    pub fn verifies(&self, proof: Option<&TuningOwnerProof>) -> bool {
        let (Some(expected), Some(proof)) = (self.0, proof) else {
            return false;
        };
        let actual = proof.digest();
        expected
            .iter()
            .zip(actual)
            .fold(0, |diff, (a, b)| diff | (a ^ b))
            == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::format;

    #[test]
    fn ownership_is_private_non_rebindable_and_fails_closed() {
        let owner = TuningOwnerProof::from_random_bytes([0x41; 32]);
        let other = TuningOwnerProof::from_random_bytes([0x42; 32]);
        let mut verifier = TuningOwnerVerifier::empty();
        assert!(!verifier.verifies(Some(&owner)));
        assert!(verifier.bind(&owner));
        assert!(!verifier.bind(&other));
        assert!(!verifier.verifies(None));
        assert!(!verifier.verifies(Some(&other)));
        assert!(verifier.verifies(Some(&owner)));
        assert_eq!(format!("{owner:?}"), "<redacted>");
        assert_eq!(format!("{verifier:?}"), "<redacted>");
        let encoded = serde_json::to_string(&owner).unwrap();
        assert_eq!(
            serde_json::from_str::<TuningOwnerProof>(&encoded).unwrap(),
            owner
        );
    }
}
