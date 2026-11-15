// What the phrase screen asks of the phone (identity spec 4.1): its own
// unlock before the words show, the screen kept out of screenshots and
// the recents view, and the two ways the words leave it on purpose,
// printed or saved as a file. There is no copy: clipboards sync to other
// devices, and other apps read them.
package chat.lumia

import android.app.Activity
import android.app.KeyguardManager
import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Typeface
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricPrompt
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.print.PageRange
import android.print.PrintAttributes
import android.print.PrintDocumentAdapter
import android.print.PrintDocumentInfo
import android.print.PrintManager
import android.print.pdf.PrintedPdfDocument
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import android.view.WindowManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.platform.LocalContext
import chat.lumia.kit.PHRASE_DOCUMENT_TITLE
import chat.lumia.kit.PHRASE_WARNING
import chat.lumia.kit.phraseColumns
import java.io.FileOutputStream
import java.io.IOException

/**
 * The window marked secure while the composable calling this is on
 * screen (`FLAG_SECURE`): left out of screenshots, screen recordings and
 * the recents view, and the mark taken off as it goes.
 */
@Composable
fun SecureWindow() {
    val activity = LocalContext.current as? Activity ?: return
    DisposableEffect(activity) {
        activity.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        onDispose { activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE) }
    }
}

/**
 * The phone's own unlock, as a function to call: a biometric or the
 * screen lock's PIN, pattern or password on Android 11 and later
 * (`BiometricPrompt` with the device credential allowed); on 10 the same
 * where a biometric is enrolled; else the screen lock's own prompt
 * (`KeyguardManager`'s device-credential intent). A phone with no screen
 * lock has nothing to ask, and [done] is told yes at once.
 */
@Composable
fun rememberDeviceUnlock(title: String, subtitle: String, done: (Boolean) -> Unit): () -> Unit {
    val context = LocalContext.current
    val answer by rememberUpdatedState(done)
    val keyguardPrompt = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        answer(it.resultCode == Activity.RESULT_OK)
    }
    return {
        val keyguard = context.getSystemService(KeyguardManager::class.java)
        when {
            keyguard == null || !keyguard.isDeviceSecure -> answer(true)
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.R -> biometricPrompt(context, title, subtitle) { answer(it) }
            Build.VERSION.SDK_INT == Build.VERSION_CODES.Q && biometricEnrolled(context) -> biometricPrompt(context, title, subtitle) { answer(it) }
            else -> {
                @Suppress("DEPRECATION")
                val intent = keyguard.createConfirmDeviceCredentialIntent(title, subtitle)
                if (intent == null) answer(true) else keyguardPrompt.launch(intent)
            }
        }
    }
}

@Suppress("DEPRECATION")
private fun biometricEnrolled(context: Context): Boolean =
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q &&
        context.getSystemService(BiometricManager::class.java)?.canAuthenticate() == BiometricManager.BIOMETRIC_SUCCESS

private fun biometricPrompt(context: Context, title: String, subtitle: String, done: (Boolean) -> Unit) {
    val builder = BiometricPrompt.Builder(context).setTitle(title).setSubtitle(subtitle)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        builder.setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_WEAK or BiometricManager.Authenticators.DEVICE_CREDENTIAL)
    } else {
        @Suppress("DEPRECATION")
        builder.setDeviceCredentialAllowed(true)
    }
    builder.build().authenticate(
        CancellationSignal(),
        context.mainExecutor,
        object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) = done(true)
            // A finger not recognised is tried again by the prompt itself; an error (cancelled, locked out) is its end.
            override fun onAuthenticationError(errorCode: Int, errString: CharSequence) = done(false)
        },
    )
}

/** Prints the phrase through the system's print dialog: a page of the warning and the words numbered in two columns. */
fun printPhrase(context: Context, words: List<String>) {
    val printing = context.getSystemService(PrintManager::class.java) ?: return
    printing.print(PHRASE_DOCUMENT_TITLE, PhrasePage(context, words), null)
}

/** Writes the phrase's file where the system's file picker put it; false when it could not be written. */
fun savePhrase(context: Context, uri: Uri, text: String): Boolean = runCatching {
    context.contentResolver.openOutputStream(uri, "wt")?.use { it.write(text.toByteArray()) } != null
}.getOrDefault(false)

// The printed page, in PostScript points (72 to the inch) inside the
// page's margins: the words a size a pen copies easily, the warning under
// the title as a paragraph.
private const val TITLE_PT = 20f
private const val TEXT_PT = 11f
private const val WORD_PT = 14f
private const val WORD_LINE_PT = 26f
private const val GAP_PT = 18f
private const val MARGIN_PT = 54f
/** Where a word's number ends, and where the word begins, from its column's edge. */
private const val NUMBER_END_PT = 20f
private const val WORD_AT_PT = 30f

/** One page, drawn: nothing of the phrase goes anywhere but to the print service. */
private class PhrasePage(private val context: Context, private val words: List<String>) : PrintDocumentAdapter() {
    private var attributes: PrintAttributes? = null

    override fun onLayout(old: PrintAttributes?, new: PrintAttributes, cancel: CancellationSignal, callback: LayoutResultCallback, extras: Bundle?) {
        if (cancel.isCanceled) return callback.onLayoutCancelled()
        attributes = new
        val info = PrintDocumentInfo.Builder("$PHRASE_DOCUMENT_TITLE.pdf")
            .setContentType(PrintDocumentInfo.CONTENT_TYPE_DOCUMENT)
            .setPageCount(1)
            .build()
        callback.onLayoutFinished(info, new != old)
    }

    override fun onWrite(pages: Array<out PageRange>, destination: ParcelFileDescriptor, cancel: CancellationSignal, callback: WriteResultCallback) {
        val document = PrintedPdfDocument(context, attributes ?: return callback.onWriteFailed(null))
        try {
            val page = document.startPage(0)
            draw(page.canvas, page.info.pageWidth.toFloat())
            document.finishPage(page)
            if (cancel.isCanceled) return callback.onWriteCancelled()
            FileOutputStream(destination.fileDescriptor).use { document.writeTo(it) }
            callback.onWriteFinished(arrayOf(PageRange.ALL_PAGES))
        } catch (e: IOException) {
            callback.onWriteFailed(e.message)
        } finally {
            document.close()
        }
    }

    private fun draw(canvas: Canvas, pageWidth: Float) {
        val width = pageWidth - 2 * MARGIN_PT
        var y = MARGIN_PT
        val title = Paint(Paint.ANTI_ALIAS_FLAG).apply { textSize = TITLE_PT; typeface = Typeface.DEFAULT_BOLD }
        y += TITLE_PT
        canvas.drawText(PHRASE_DOCUMENT_TITLE, MARGIN_PT, y, title)
        y += GAP_PT
        val text = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { textSize = TEXT_PT }
        val warning = StaticLayout.Builder.obtain(PHRASE_WARNING, 0, PHRASE_WARNING.length, text, width.toInt())
            .setAlignment(Layout.Alignment.ALIGN_NORMAL)
            .build()
        canvas.save()
        canvas.translate(MARGIN_PT, y)
        warning.draw(canvas)
        canvas.restore()
        y += warning.height + GAP_PT * 1.5f
        val number = Paint(Paint.ANTI_ALIAS_FLAG).apply { textSize = TEXT_PT; typeface = Typeface.DEFAULT_BOLD; textAlign = Paint.Align.RIGHT }
        val word = Paint(Paint.ANTI_ALIAS_FLAG).apply { textSize = WORD_PT; typeface = Typeface.MONOSPACE }
        phraseColumns(words).forEachIndexed { column, entries ->
            val x = MARGIN_PT + column * width / 2
            entries.forEachIndexed { row, (n, w) ->
                val baseline = y + (row + 1) * WORD_LINE_PT
                canvas.drawText("$n", x + NUMBER_END_PT, baseline, number)
                canvas.drawText(w, x + WORD_AT_PT, baseline, word)
            }
        }
    }
}
