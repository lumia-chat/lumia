//! Android's TLS: the core's HTTPS (reqwest's default verifier and the link-card fetcher's)
//! trusts the system's certificates through `rustls-platform-verifier`, which on Android calls
//! the platform's trust manager over JNI and must be handed the JVM and the application's
//! `Context` once, before the first request. `chat.lumia.kit.Tls.initialize(context)` does it
//! at the app's start; without it every HTTPS request panics in the verifier.
//!
//! The gateway's WebSocket is tokio-tungstenite with webpki's roots (Mozilla's), not this.

// A JNI entry point is an exported, unmangled symbol the JVM resolves by name, which the
// `unsafe_code` lint counts as unsafe; nothing else here is.
#![allow(unsafe_code)]

use jni::errors::ThrowRuntimeExAndDefault;
use jni::jni_mangle;
use jni::objects::JObject;
use jni::EnvUnowned;

/// `Tls.initialize(context)`: hands the verifier the JVM and the application's `Context` (and
/// through it the class loader that finds the verifier's Kotlin component). Once per process
/// is enough; a second call keeps the first's. A failure is thrown as a `RuntimeException`.
#[jni_mangle("chat.lumia.kit.Tls")]
pub extern "system" fn initialize<'local>(
    mut env: EnvUnowned<'local>,
    _this: JObject<'local>,
    context: JObject<'local>,
) {
    env.with_env(|env| rustls_platform_verifier::android::init_with_env(env, context))
        .resolve::<ThrowRuntimeExAndDefault>();
}
