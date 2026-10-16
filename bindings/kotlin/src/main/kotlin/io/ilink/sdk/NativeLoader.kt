package io.ilink.sdk

import java.io.IOException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.Locale

/**
 * Locates and extracts the native `ilink_sdk` library.
 *
 * The published JAR bundles the compiled library for every supported platform
 * under `io/ilink/sdk/native/<platform>/`. At runtime this object picks the
 * entry matching the current OS and CPU, extracts it to a temporary directory
 * (once per JVM) and hands the absolute path to JNA. Nothing has to be installed
 * on `PATH` or configured with `-Djna.library.path`.
 *
 * Overrides, checked in order:
 * 1. `-Dilink.sdk.lib=/abs/path/to/libilink_sdk.so` — use this file;
 * 2. `-Dilink.sdk.lib=ilink_sdk` — let JNA resolve the name;
 * 3. the bundled resource for the current platform;
 * 4. JNA's default lookup for `ilink_sdk`.
 */
internal object NativeLoader {

    private const val OVERRIDE_PROPERTY = "ilink.sdk.lib"
    private const val RESOURCE_ROOT = "/io/ilink/sdk/native/"

    /** Resolve a value suitable for [Native.load]: an absolute path or a name. */
    fun resolve(): String {
        val override = System.getProperty(OVERRIDE_PROPERTY)?.trim()
        if (!override.isNullOrEmpty()) {
            val path = java.nio.file.Path.of(override)
            return if (Files.isRegularFile(path)) {
                path.toAbsolutePath().toString()
            } else {
                // Not a file: treat it as a library name for JNA to resolve.
                override
            }
        }
        return extractBundled() ?: libraryName()
    }

    /** The base name JNA looks up on the system loader path. */
    fun libraryName(): String = "ilink_sdk"

    private fun extractBundled(): String? {
        val platform = platformTag() ?: return null
        val resource = "$RESOURCE_ROOT$platform/${fileName()}"
        return try {
            NativeLoader::class.java.getResourceAsStream(resource)?.use { input ->
                val dir = Files.createTempDirectory("ilink-sdk-")
                dir.toFile().deleteOnExit()
                val target = dir.resolve(fileName())
                Files.copy(input, target, StandardCopyOption.REPLACE_EXISTING)
                target.toFile().deleteOnExit()
                target.toAbsolutePath().toString()
            }
        } catch (e: IOException) {
            null
        }
    }

    private fun fileName(): String {
        val os = System.getProperty("os.name", "").lowercase(Locale.ROOT)
        return when {
            os.contains("win") -> "ilink_sdk.dll"
            os.contains("mac") || os.contains("darwin") -> "libilink_sdk.dylib"
            else -> "libilink_sdk.so"
        }
    }

    /** The resource directory name for the current OS/CPU, or `null`. */
    fun platformTag(): String? {
        val os = System.getProperty("os.name", "").lowercase(Locale.ROOT)
        val arch = System.getProperty("os.arch", "").lowercase(Locale.ROOT)
        val aarch64 = arch.contains("aarch64") || arch.contains("arm64")

        return when {
            os.contains("win") -> if (aarch64) "windows-aarch64" else "windows-x86_64"
            os.contains("mac") || os.contains("darwin") ->
                if (aarch64) "macos-aarch64" else "macos-x86_64"
            os.contains("linux") -> if (aarch64) "linux-aarch64" else "linux-x86_64"
            else -> null
        }
    }
}
