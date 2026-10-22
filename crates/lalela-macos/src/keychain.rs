//! `KeychainSecretStore`: API keys in the Keychain, with environment override
//! (05-persistence.md §API keys).

use lalela_core::ports::{KeySource, ProviderId, SecretError, SecretStore, StoredKey};
use lalela_core::units::ApiKey;

/// The Keychain service name.
pub const KEYCHAIN_SERVICE: &str = "io.senzo.lalela";

/// `errSecItemNotFound` from the Security framework (-25300).
const ERRSEC_ITEM_NOT_FOUND: i32 = -25_300;

/// The environment variables that override the Keychain.
fn env_var_for(provider: ProviderId) -> Option<&'static str> {
    match provider {
        ProviderId::MistralVoxtral => Some("MISTRAL_API_KEY"),
        ProviderId::Deepgram => Some("DEEPGRAM_API_KEY"),
        ProviderId::AppleSpeech => None,
    }
}

/// A Keychain-backed secret store.
#[derive(Debug, Default)]
pub struct KeychainSecretStore;

impl KeychainSecretStore {
    /// Build the store.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl SecretStore for KeychainSecretStore {
    fn get(&self, provider: ProviderId) -> Result<Option<StoredKey>, SecretError> {
        if let Some(var) = env_var_for(provider)
            && let Ok(value) = std::env::var(var)
        {
            let key = ApiKey::try_new(value).ok_or(SecretError::Invalid)?;
            return Ok(Some(StoredKey {
                key,
                source: KeySource::Environment,
            }));
        }
        let account = provider.as_str();
        let bytes = security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, account);
        match bytes {
            Ok(bytes) => {
                let value = String::from_utf8(bytes).map_err(|_| SecretError::Invalid)?;
                let key = ApiKey::try_new(value).ok_or(SecretError::Invalid)?;
                Ok(Some(StoredKey {
                    key,
                    source: KeySource::Keychain,
                }))
            }
            Err(error) if error.code() == ERRSEC_ITEM_NOT_FOUND => Ok(None),
            Err(_) => Err(SecretError::Platform { status: 1 }),
        }
    }

    fn set(&self, provider: ProviderId, key: &ApiKey) -> Result<(), SecretError> {
        security_framework::passwords::set_generic_password(
            KEYCHAIN_SERVICE,
            provider.as_str(),
            key.as_secret().as_bytes(),
        )
        .map_err(|_| SecretError::Denied)
    }

    fn delete(&self, provider: ProviderId) -> Result<(), SecretError> {
        match security_framework::passwords::delete_generic_password(
            KEYCHAIN_SERVICE,
            provider.as_str(),
        ) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == ERRSEC_ITEM_NOT_FOUND => Ok(()),
            Err(_) => Err(SecretError::Platform { status: 1 }),
        }
    }
}
