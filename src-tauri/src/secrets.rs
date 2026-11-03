//! Tokens, in the system keychain under `dev.rostery.app` and nowhere else: not in the database, not in a
//! file, and never sent to the webview.

use rostery_core::Error;

const SERVICE: &str = "dev.rostery.app";

fn entry(name: &str) -> Result<keyring::Entry, Error> {
    keyring::Entry::new(SERVICE, name).map_err(unavailable)
}

fn unavailable(e: keyring::Error) -> Error {
    Error::Secrets(format!(
        "Rostery could not use the system keychain: {e}. On Linux it needs a Secret Service, such as \
         GNOME Keyring or KWallet, running and unlocked."
    ))
}

pub fn get(name: &str) -> Result<Option<String>, Error> {
    match entry(name)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(unavailable(e)),
    }
}

pub fn set(name: &str, value: &str) -> Result<(), Error> {
    entry(name)?.set_password(value).map_err(unavailable)
}

/// Gone already is fine.
pub fn delete(name: &str) -> Result<(), Error> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(unavailable(e)),
    }
}

#[cfg(test)]
mod tests {
    /// Writes and removes one throwaway entry in the real keychain, so it only runs when asked for.
    #[test]
    #[ignore = "uses the real system keychain"]
    fn live_keychain_round_trip() {
        let name = "rostery-selftest";
        super::set(name, "it works").unwrap();
        assert_eq!(super::get(name).unwrap().as_deref(), Some("it works"));
        super::delete(name).unwrap();
        assert_eq!(super::get(name).unwrap(), None);
        super::delete(name).unwrap();
    }
}
