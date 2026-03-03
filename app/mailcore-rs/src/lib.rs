#![deny(clippy::all)]
// Note: #![forbid(unsafe_code)] is incompatible with napi-rs macros which internally
// use allow(unsafe_code). We use #![deny(unsafe_code)] instead, which prevents us from
// writing unsafe code while allowing napi-rs macro expansions to work correctly.
#![deny(unsafe_code)]

use napi::bindgen_prelude::*;
use napi_derive::napi;

pub mod provider;
pub mod imap;

/// The embedded provider database — included at compile time from resources/providers.json.
///
/// This eliminates runtime path resolution issues across dev / production / packaged Electron.
static PROVIDERS_JSON: &str = include_str!("../resources/providers.json");

/// napi module initializer — called automatically when the .node addon is loaded.
///
/// Parses the embedded providers.json and stores providers in the global singleton.
#[napi(module_exports)]
pub fn module_init(mut _exports: Object) -> Result<()> {
    provider::init_from_embedded(PROVIDERS_JSON)?;
    Ok(())
}
