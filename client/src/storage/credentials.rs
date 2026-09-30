//! Credential-provider mechanics only. The parent worker owns ordering and rollback.
#[cfg(not(target_os = "linux"))]
compile_error!("Persistent sessions require the Linux Secret Service backend");

use super::Selection;

pub(super) const SERVICE: &str = "org.hamlet.session.v1";

// No debug output of store errors: backend errors may include implementation-specific data.
pub(crate) trait Store: Send + 'static {
    fn get(&mut self, selection: &Selection) -> Result<Option<String>, ()>;
    fn put(&mut self, selection: &Selection, token: &str) -> Result<(), ()>;
    fn delete(&mut self, selection: &Selection) -> Result<(), ()>;
}

pub(crate) fn account(selection: &Selection) -> String {
    // Length prefix prevents ambiguous server/user pairings; the URL is already validated.
    // Preserve its original spelling, not the HTTP client's normalized URL.
    format!(
        "{}:{}:{}",
        selection.server.len(),
        selection.server,
        selection.user.id
    )
}

#[cfg(not(test))]
pub(super) struct SecretService;
#[cfg(not(test))]
impl SecretService {
    fn entry(selection: &Selection) -> Result<keyring::Entry, ()> {
        keyring::Entry::new(SERVICE, &account(selection)).map_err(|_| ())
    }
}
#[cfg(not(test))]
impl Store for SecretService {
    fn get(&mut self, s: &Selection) -> Result<Option<String>, ()> {
        match Self::entry(s)?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(()),
        }
    }
    fn put(&mut self, s: &Selection, token: &str) -> Result<(), ()> {
        Self::entry(s)?.set_password(token).map_err(|_| ())
    }
    fn delete(&mut self, s: &Selection) -> Result<(), ()> {
        match Self::entry(s)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(()),
        }
    }
}
