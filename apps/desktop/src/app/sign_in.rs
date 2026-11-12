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
    /// A magic link was asked for (architecture 6.3): the card says where it went and
    /// takes its code.
    pub magic_sent: bool,
    pub magic_code: TextEdit,
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
            magic_sent: false,
            magic_code: TextEdit::new(""),
        }
    }

    /// Forgets a magic link that was asked for.
    fn forget_magic_link(&mut self) {
        self.magic_sent = false;
        self.magic_code.clear();
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
        self.sign_in.forget_magic_link();
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
                self.sign_in.forget_magic_link();
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

/// The magic link (architecture 6.3): a sign-in link mailed to the address in the handle
/// field, completed here by its code, or by the whole link pasted.
impl State {
    /// Asks the instance to mail a sign-in link to the address in the handle field.
    pub fn request_magic_link(&mut self) {
        let form = &mut self.sign_in;
        if form.busy || form.handle.text.trim().is_empty() {
            return;
        }
        form.error.clear();
        form.busy = true;
        self.session.request_magic_link(&form.handle.text);
    }

    /// Signs in with the code from the mail.
    pub fn complete_magic_link(&mut self) {
        let form = &mut self.sign_in;
        if form.busy || form.magic_code.text.trim().is_empty() {
            return;
        }
        form.error.clear();
        form.busy = true;
        self.session.sign_in_magic_link(&form.magic_code.text);
    }

    /// The instance answered a request for a link.
    pub(super) fn magic_link_sent(&mut self, answer: &Result<(), Failure>) {
        let form = &mut self.sign_in;
        form.busy = false;
        match answer {
            Ok(()) => form.magic_sent = true,
            Err(failure) => {
                form.error = match failure.code.as_str() {
                    "RATE_LIMITED" => "Too many links in the last hour; try later.".into(),
                    _ => failure.message.clone(),
                }
            }
        }
    }

    /// The instance answered a magic link's code.
    pub(super) fn magic_link_signed_in(&mut self, answer: &Result<(), Failure>) {
        match answer {
            Ok(()) => self.signed_in(&Ok(())),
            Err(failure) => {
                let form = &mut self.sign_in;
                form.busy = false;
                form.error = match failure.code.as_str() {
                    "UNAUTHENTICATED" => {
                        "That code is unknown, used or expired. Ask for a new link.".into()
                    }
                    _ => failure.message.clone(),
                };
            }
        }
    }
}
