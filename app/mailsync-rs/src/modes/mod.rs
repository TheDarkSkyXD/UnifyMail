// Modes module — process mode handlers for mailsync-rs.
//
// Each mode corresponds to a --mode flag value and implements a distinct
// binary behavior. Modes are dispatched in main.rs based on the Mode enum.

pub mod install_check;
pub mod migrate;
pub mod reset;
pub mod sync;
pub mod test_auth;
