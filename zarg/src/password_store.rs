const SERVICE: &str = "zarg";
const USERNAME: &str = "default-archive-password";

fn entry() -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, USERNAME)
}

pub(crate) fn load_default_password() -> keyring::Result<Option<String>> {
    match entry()?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(crate) fn save_default_password(password: &str) -> keyring::Result<()> {
    entry()?.set_password(password)
}

pub(crate) fn remove_default_password() -> keyring::Result<()> {
    match entry()?.delete_credential() {
        Err(keyring::Error::NoEntry) => Ok(()),
        result => result,
    }
}
