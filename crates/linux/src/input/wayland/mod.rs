pub(crate) mod kept_session;
pub(crate) mod key_session;
pub(crate) mod keysym_send;
pub(crate) mod layout;
pub(crate) mod pointer_send;
pub(crate) mod remote_desktop;
pub(crate) mod virtual_keyboard;

#[cfg(test)]
mod fake_buffering_session;
#[cfg(test)]
mod fake_session;
#[cfg(test)]
mod live_shell;
#[cfg(test)]
mod live_tests;
