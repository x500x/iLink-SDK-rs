package io.ilink.sdk;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Locale;

/**
 * Locates and extracts the native {@code ilink_sdk} library.
 *
 * <p>The published JAR bundles the compiled library for every supported
 * platform under {@code /io/ilink/sdk/native/<platform>/}. At runtime this class
 * picks the entry matching the current OS and CPU, extracts it to a temporary
 * directory (once per JVM) and hands the absolute path to JNA. Nothing has to be
 * installed on {@code PATH} or configured with {@code -Djna.library.path}.
 *
 * <p>Overrides, checked in order:
 * <ol>
 *   <li>{@code -Dilink.sdk.lib=/abs/path/to/libilink_sdk.so} — use this file;</li>
 *   <li>{@code -Dilink.sdk.lib=ilink_sdk} — let JNA resolve the name
 *       ({@code jna.library.path}, {@code PATH}, ...);</li>
 *   <li>the bundled resource for the current platform;</li>
 *   <li>JNA's default lookup for {@code ilink_sdk}.</li>
 * </ol>
 */
final class NativeLoader {

    private static final String OVERRIDE_PROPERTY = "ilink.sdk.lib";
    private static final String RESOURCE_ROOT = "/io/ilink/sdk/native/";

    private NativeLoader() {}

    /** Resolve a value suitable for {@code Native.load}: an absolute path or a name. */
    static String resolve() {
        String override = System.getProperty(OVERRIDE_PROPERTY);
        if (override != null && !override.trim().isEmpty()) {
            Path path = Path.of(override.trim());
            if (Files.isRegularFile(path)) {
                return path.toAbsolutePath().toString();
            }
            // Not a file: treat it as a library name for JNA to resolve.
            return override.trim();
        }
        String extracted = extractBundled();
        return extracted != null ? extracted : libraryName();
    }

    /** The base name JNA looks up on the system loader path. */
    static String libraryName() {
        return "ilink_sdk";
    }

    private static String extractBundled() {
        String platform = platformTag();
        if (platform == null) {
            return null;
        }
        String resource = RESOURCE_ROOT + platform + "/" + fileName();
        try (InputStream in = NativeLoader.class.getResourceAsStream(resource)) {
            if (in == null) {
                return null;
            }
            Path dir = Files.createTempDirectory("ilink-sdk-");
            dir.toFile().deleteOnExit();
            Path target = dir.resolve(fileName());
            Files.copy(in, target, StandardCopyOption.REPLACE_EXISTING);
            target.toFile().deleteOnExit();
            return target.toAbsolutePath().toString();
        } catch (IOException e) {
            return null;
        }
    }

    private static String fileName() {
        String os = System.getProperty("os.name", "").toLowerCase(Locale.ROOT);
        if (os.contains("win")) {
            return "ilink_sdk.dll";
        }
        if (os.contains("mac") || os.contains("darwin")) {
            return "libilink_sdk.dylib";
        }
        return "libilink_sdk.so";
    }

    /** The resource directory name for the current OS/CPU, or {@code null}. */
    static String platformTag() {
        String os = System.getProperty("os.name", "").toLowerCase(Locale.ROOT);
        String arch = System.getProperty("os.arch", "").toLowerCase(Locale.ROOT);
        boolean aarch64 = arch.contains("aarch64") || arch.contains("arm64");

        if (os.contains("win")) {
            return aarch64 ? "windows-aarch64" : "windows-x86_64";
        }
        if (os.contains("mac") || os.contains("darwin")) {
            return aarch64 ? "macos-aarch64" : "macos-x86_64";
        }
        if (os.contains("linux")) {
            return aarch64 ? "linux-aarch64" : "linux-x86_64";
        }
        return null;
    }
}
