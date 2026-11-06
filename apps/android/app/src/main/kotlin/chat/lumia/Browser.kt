// A sign-in through the system browser (architecture 6.3): the instance's
// OIDC provider, or its web sign-in page for a passkey, in a Custom Tab
// that comes back to the app on the `lumia:` scheme with a one-time token
// (the manifest's intent filter; MainActivity hands the URL to the store).
// Platform passkeys through the Credential Manager would need the
// instance's host to serve an asset link naming this app's signing key,
// which an app for any instance cannot count on; the page in the tab is
// the browser's own WebAuthn, with no such tie.
package chat.lumia

import android.content.Context
import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent

fun openInBrowser(context: Context, url: String) {
    CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(url))
}
