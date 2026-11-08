//! Checking who's signed in, signing in, and signing out.

use super::{Done, Login};
use crate::jobs::Done as Job;
use crate::state::App;
use rune::Tx;
use std::time::{Duration, Instant};
use tgg_account::{Error, Poll, SignedIn};

/// How much longer to wait between polls when the site says to slow down.
const SLOW_DOWN: Duration = Duration::from_secs(5);

/// Ask the site who's signed in (the saved sign-in, when it can't be
/// reached).
pub fn check(app: &mut App) {
    let site = app.signin.site.clone();
    app.jobs.spawn("whoami", move || {
        Job::SignIn(Box::new(Done::Checked(site.whoami())))
    });
}

/// Start signing in: ask for a code, and show the dialog.
pub fn begin(app: &mut App) {
    app.signin.account_open = false;
    app.signin.login = Some(Login::asking());
    let site = app.signin.site.clone();
    app.jobs.spawn("sign in", move || {
        Job::SignIn(Box::new(Done::Begun(site.begin_device_login())))
    });
}

/// Stop signing in. A poll on its way is ignored when it arrives.
pub fn cancel(app: &mut App) {
    app.signin.login = None;
}

/// Open the page the code is confirmed on.
pub fn open_page(app: &mut App, tx: &mut Tx) {
    let Some(link) = app
        .signin
        .login
        .as_ref()
        .and_then(|login| login.link.clone())
    else {
        return;
    };
    if let Err(error) = tx.open_url(&link) {
        app.tell(format!("Couldn't open {link}: {error}"), true);
    }
}

/// Copy the code, to paste on the site.
pub fn copy_code(app: &mut App, tx: &mut Tx) {
    if let Some(code) = app
        .signin
        .login
        .as_ref()
        .and_then(|login| login.code.clone())
    {
        tx.copy(code);
    }
}

/// Before each frame: ask whether the code's confirmed, when it's time.
pub fn tick(app: &mut App) {
    let Some(login) = &mut app.signin.login else {
        return;
    };
    let due = login.next.is_some_and(|next| Instant::now() >= next);
    if !due || login.polling {
        return;
    }
    let Some(device) = login.device.clone() else {
        return;
    };
    login.polling = true;
    login.next = None;
    let site = app.signin.site.clone();
    app.jobs.spawn("sign in", move || {
        Job::SignIn(Box::new(Done::Polled(site.poll_device_login(&device))))
    });
}

/// A job finished. True: what shows changed.
pub fn done(app: &mut App, done: Done, tx: &mut Tx) -> bool {
    match done {
        Done::Checked(result) => match result {
            Ok(who) => app.signin.who = who,
            Err(Error::SessionExpired) => {
                app.signin.who = None;
                app.tell("Your textures.gg sign-in expired; sign in again.", true);
            }
            Err(error) => tgg_app::log(&format!("checking the sign-in: {error}")),
        },
        Done::Begun(result) => {
            let Some(login) = &mut app.signin.login else {
                return false;
            };
            match result {
                Ok(device) => {
                    login.code = Some(device.display_code());
                    login.link = Some(device.link().to_owned());
                    login.expires = Some(device.expires_at);
                    login.interval = device.interval;
                    login.next = Some(Instant::now() + device.interval);
                    tx.wake_after(device.interval);
                    login.device = Some(device);
                }
                Err(error) => login.problem = Some(sentence(&error)),
            }
        }
        Done::Polled(result) => polled(app, result, tx),
        Done::SignedOut(result) => {
            app.signin.who = None;
            app.signin.account_open = false;
            match result {
                Ok(()) => app.tell("Signed out of textures.gg.", false),
                Err(error) => app.tell(format!("Couldn't sign out: {error}."), true),
            }
        }
    }
    true
}

/// The answer to a poll: wait and ask again, or it's done, or it's over.
pub fn polled(app: &mut App, result: Result<Poll, Error>, tx: &mut Tx) {
    let Some(login) = &mut app.signin.login else {
        return;
    };
    login.polling = false;
    match result {
        Ok(Poll::Pending) => {
            login.next = Some(Instant::now() + login.interval);
            tx.wake_after(login.interval);
        }
        Ok(Poll::SlowDown) => {
            login.interval += SLOW_DOWN;
            login.next = Some(Instant::now() + login.interval);
            tx.wake_after(login.interval);
        }
        Ok(Poll::Done(account)) => {
            app.tell(format!("Signed in as {}.", account.handle), false);
            app.signin.who = Some(SignedIn {
                account,
                verified: true,
            });
            app.signin.login = None;
        }
        Ok(Poll::Expired) => {
            login.problem =
                Some("The code expired before it was confirmed. Start again for a new one.".into())
        }
        Ok(Poll::Denied) => {
            login.problem = Some("The sign-in was cancelled on textures.gg.".into())
        }
        Err(error) => login.problem = Some(sentence(&error)),
    }
}

/// Show or hide who's signed in, and signing out.
pub fn show_account(app: &mut App, shown: bool) {
    app.signin.account_open = shown;
}

pub fn hide_account(app: &mut App) {
    show_account(app, false);
}

/// Sign out, here and in the CLI (they share the sign-in).
pub fn sign_out(app: &mut App) {
    let site = app.signin.site.clone();
    app.jobs.spawn("sign out", move || {
        Job::SignIn(Box::new(Done::SignedOut(site.logout().map(|_| ()))))
    });
}

/// The sidebar's account row: who's signed in, else signing in.
pub fn account_row(app: &mut App) {
    if app.signin.who.is_some() {
        show_account(app, true);
    } else {
        begin(app);
    }
}

/// `error` as a sentence on its own.
fn sentence(error: &Error) -> String {
    let words = error.to_string();
    let mut characters = words.chars();
    let mut out: String = characters
        .next()
        .map(|first| first.to_uppercase().chain(characters).collect())
        .unwrap_or_default();
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}
