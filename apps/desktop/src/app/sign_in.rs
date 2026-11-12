//! The sign-in screen's form and its reactions: the instance, then an account on it.

use rune::TextEdit;

use super::{Route, State};
use crate::session::Failure;

/// The sign-in screen: the instance, then an account on it.
#[derive(Debug)]
pub struct SignIn {
    pub instance: TextEdit,
    pub handle: TextEdit,
    pub password: TextEdit,
    pub totp: TextEdit,
    /// A new account instead of an existing one.
    pub creating: bool,
    /// The account asked for its second factor.
    pub needs_totp: bool,
    /// Waiting for the instance's trust card.
    pub reaching: bool,
    /// Waiting for the instance's answer to a sign-in.
    pub busy: bool,
    pub error: String,
}

impl SignIn {
    pub(super) fn new(instance: &str) -> Self {
        Self {
            instance: TextEdit::new(instance),
            handle: TextEdit::new(""),
            password: TextEdit::new("").secure(),
            totp: TextEdit::new(""),
            creating: false,
            needs_totp: false,
            reaching: false,
            busy: false,
            error: String::new(),
        }
    }
}

impl State {
    /// Opens the instance in the field and asks for its trust card.
    pub fn connect(&mut self) {
        self.sign_in.error.clear();
        let url = self
            .sign_in
            .instance
            .text
            .trim()
            .trim_end_matches('/')
            .to_string();
        match self.session.open(&url) {
            Ok(()) => {
                self.sign_in.reaching = true;
                self.settings.instance = Some(url);
                self.settings.save(&self.data_dir);
            }
            Err(error) => self.sign_in.error = error,
        }
    }

    /// Back to the instance field.
    pub fn change_instance(&mut self) {
        self.session.close();
        self.sign_in.error.clear();
        self.sign_in.needs_totp = false;
        self.sign_in.busy = false;
    }

    /// Signs in, or creates the account, with what the form holds.
    pub fn submit(&mut self) {
        let form = &mut self.sign_in;
        if form.busy || form.handle.text.trim().is_empty() || form.password.text.is_empty() {
            return;
        }
        form.error.clear();
        form.busy = true;
        if form.creating {
            self.session
                .create_account(&form.handle.text, &form.password.text);
        } else {
            let code = form.needs_totp.then_some(form.totp.text.trim());
            self.session
                .sign_in(&form.handle.text, &form.password.text, code);
        }
    }

    pub fn sign_out(&mut self) {
        self.session.sign_out();
        self.leave_shell();
        self.route = Route::SignIn;
    }

    /// The instance answered a sign-in.
    pub(super) fn signed_in(&mut self, answer: &Result<(), Failure>) {
        self.sign_in.busy = false;
        match answer {
            Ok(()) => {
                self.sign_in.password.clear();
                self.sign_in.totp.clear();
                self.sign_in.needs_totp = false;
                self.route = Route::Shell;
            }
            Err(failure) => self.refused(failure),
        }
    }

    /// The sign-in screen's wording for a refusal, as the other shells word it.
    fn refused(&mut self, failure: &Failure) {
        let form = &mut self.sign_in;
        let mfa = failure.details.get("mfa").map(String::as_str);
        form.error = match failure.code.as_str() {
            // The passkey factor needs the browser's ceremony (architecture 6.3), which
            // this shell does not hand off to yet.
            "FAILED_PRECONDITION" if mfa == Some("passkey") => {
                "This account asks for its passkey at sign-in, which this app cannot do yet. Sign in from the web for now.".into()
            }
            "FAILED_PRECONDITION" if !form.needs_totp && !form.creating => {
                form.needs_totp = true;
                String::new()
            }
            "UNAUTHENTICATED" if form.needs_totp => "That code was not accepted.".into(),
            "UNAUTHENTICATED" => "Wrong handle or password.".into(),
            "ALREADY_EXISTS" => "That handle is taken.".into(),
            _ => failure.message.clone(),
        };
    }
}
