package io.ilink.sdk

import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer

/**
 * An iLink SDK operation failed. [kind] mirrors `ilink_sdk::ErrorKind`
 * (`dns`, `tcp`, `tls`, `timeout`, `cancelled`, ...).
 */
class IlinkError(val kind: String, message: String) : RuntimeException(message)

/** JNA view of the exported C symbols. Public so JNA can proxy it by name. */
interface IlinkNativeLibrary : Library {
    fun ilink_call(op: String, paramsJson: String?): Pointer?
    fun ilink_version(): Pointer?
    fun ilink_string_free(pointer: Pointer?)
}

/**
 * Low-level access to the Rust library's JSON C ABI.
 *
 * Everything goes through [call]: pass an operation name and a JSON parameter
 * object, get the operation `result` JSON back. Failures throw [IlinkError].
 *
 * The native library is bundled in the JAR and extracted at runtime (see
 * [NativeLoader]), so no manual setup is required. Override the lookup with
 * `-Dilink.sdk.lib=<path-or-name>`, or point JNA at a directory with
 * `-Djna.library.path=<dir>`.
 */
object IlinkSdk {

    private val lib: IlinkNativeLibrary =
        Native.load(NativeLoader.resolve(), IlinkNativeLibrary::class.java)

    private fun take(pointer: Pointer?): String {
        if (pointer == null) throw IlinkError("unknown", "the SDK returned a NULL string")
        return try {
            pointer.getString(0, "UTF-8")
        } finally {
            lib.ilink_string_free(pointer)
        }
    }

    /** The SDK version string. */
    @JvmStatic
    fun version(): String = take(lib.ilink_version())

    /** Run one SDK operation and return the raw `result` JSON. */
    @JvmStatic
    fun call(op: String, paramsJson: String?): String {
        val envelope = take(lib.ilink_call(op, paramsJson))
        if (Json.field(envelope, "ok") == "true") {
            return Json.field(envelope, "result") ?: "null"
        }
        val message = Json.decodeString(Json.field(envelope, "error")) ?: "operation failed"
        val kind = Json.decodeString(Json.field(envelope, "kind")) ?: "unknown"
        throw IlinkError(kind, message)
    }

    /** Run one SDK operation and return the `result` parsed as JSON. */
    @JvmStatic
    fun callValue(op: String, paramsJson: String? = "{}"): JsonValue =
        JsonValue.parse(call(op, paramsJson))
}
