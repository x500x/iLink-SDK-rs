package io.ilink.sdk;

import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.Pointer;

/**
 * Low-level access to the Rust library's JSON C ABI.
 *
 * <p>Everything goes through {@link #call(String, String)}: pass an operation
 * name and a JSON parameter object, get the operation {@code result} JSON back.
 * Failures throw {@link IlinkError}.
 *
 * <p>The shared library is located through JNA: set {@code -Djna.library.path}
 * to the directory containing {@code ilink_sdk.dll} / {@code libilink_sdk.so},
 * or set {@code -Dilink.sdk.lib} to the exact library name. On Windows the DLL
 * must also be on {@code PATH} (or next to the JVM launcher).
 */
public final class IlinkSdk {

    private IlinkSdk() {}

    /** JNA view of the exported C symbols. */
    interface NativeLibrary extends Library {
        Pointer ilink_call(String op, String paramsJson);

        Pointer ilink_version();

        void ilink_string_free(Pointer ptr);
    }

    private static final NativeLibrary LIB = Native.load(
            System.getProperty("ilink.sdk.lib", "ilink_sdk"), NativeLibrary.class);

    private static String take(Pointer pointer) {
        if (pointer == null) {
            throw new IlinkError("unknown", "the SDK returned a NULL string");
        }
        try {
            return pointer.getString(0, "UTF-8");
        } finally {
            LIB.ilink_string_free(pointer);
        }
    }

    /** The SDK version string. */
    public static String version() {
        return take(LIB.ilink_version());
    }

    /**
     * Run one SDK operation and return the raw {@code result} JSON.
     *
     * <pre>{@code
     * String info = IlinkSdk.call("mime.info", "{\"path\":\"photo.png\"}");
     * }</pre>
     */
    public static String call(String op, String paramsJson) {
        String envelope = take(LIB.ilink_call(op, paramsJson));
        if ("true".equals(Json.field(envelope, "ok"))) {
            String result = Json.field(envelope, "result");
            return result == null ? "null" : result;
        }
        String message = Json.decodeString(Json.field(envelope, "error"));
        String kind = Json.decodeString(Json.field(envelope, "kind"));
        throw new IlinkError(kind, message == null ? "operation failed" : message);
    }

    /**
     * Run one SDK operation and return the {@code result} parsed as JSON.
     *
     * <pre>{@code
     * JsonValue info = IlinkSdk.callValue("mime.info", "{\"path\":\"photo.png\"}");
     * String mime = info.get("mime").asString("");
     * }</pre>
     */
    public static JsonValue callValue(String op, String paramsJson) {
        return JsonValue.parse(call(op, paramsJson));
    }
}
