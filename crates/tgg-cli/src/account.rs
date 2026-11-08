//! `tgg login` and `tgg logout`, and requests to the site's API as the
//! signed-in user, over tgg-account (which tgg-studio shares, so both use
//! one sign-in).

use anyhow::Result;
use serde::de::DeserializeOwned;
use serde_json::Value;

const USER_AGENT: &str = concat!("tgg/", env!("CARGO_PKG_VERSION"));

/// The textures.gg API, as the signed-in user if there is one.
pub struct Site(tgg_account::Site);

impl Site {
    pub fn new(api: &str) -> Result<Self> {
        Ok(Self(tgg_account::Site::new(api, USER_AGENT).map_err(cli)?))
    }

    /// Check the saved sign-in still works, so a command that acts as the
    /// user can say so instead of failing on whatever it asks for first.
    pub fn signed_in(&self) -> Result<&Self> {
        self.0.signed_in().map_err(cli)?;
        Ok(self)
    }

    /// Whether a path of the API answers the signed-in user with something,
    /// rather than 404.
    pub fn exists(&self, path: &str) -> Result<bool> {
        self.0.exists(path).map_err(cli)
    }

    /// POST JSON to a path of the API as the signed-in user.
    pub fn post<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        self.0.post(path, body).map_err(cli)
    }
}

/// tgg-account's errors, with the command to run where there is one.
fn cli(error: tgg_account::Error) -> anyhow::Error {
    use tgg_account::Error;
    match error {
        Error::NotSignedIn => anyhow::anyhow!("you're not signed in; run tgg login"),
        Error::SessionExpired => {
            anyhow::anyhow!("your sign-in has expired or was removed; run tgg login")
        }
        Error::Expired => anyhow::anyhow!("the code expired; run tgg login again"),
        Error::Offline { api, reason } => anyhow::anyhow!("reaching {api}: {reason}"),
        error => error.into(),
    }
}

/// `tgg login`: confirm a code on the site and save the token it grants.
pub fn login(api: &str, browser: bool) -> Result<()> {
    let site = Site::new(api)?;
    let account = site
        .0
        .login(false, |code| {
            println!(
                "Your code is {}. Confirm it on textures.gg:",
                code.display_code()
            );
            println!("  {}", code.link());
            if browser && open::that_detached(code.link()).is_err() {
                println!("(Open that link in a browser.)");
            }
        })
        .map_err(cli)?;
    println!("Signed in as {}.", account.handle);
    Ok(())
}

/// `tgg logout`: end the session on the site and forget its token.
pub fn logout(api: &str) -> Result<()> {
    match Site::new(api)?.0.logout().map_err(cli)? {
        Some(account) => println!("Signed out {}.", account.handle),
        None => println!("You're not signed in."),
    }
    Ok(())
}
