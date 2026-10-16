import java.util.List;

import io.ilink.sdk.IlinkClient;
import io.ilink.sdk.IlinkError;
import io.ilink.sdk.IlinkModels.Account;
import io.ilink.sdk.IlinkModels.CryptoResult;
import io.ilink.sdk.IlinkModels.MimeInfo;
import io.ilink.sdk.IlinkSdk;
import io.ilink.sdk.JsonValue;

/**
 * Minimal Java consumer / smoke test of the iLink SDK.
 *
 * <p>Build the library first (repository root): {@code cargo build --release}
 * Then compile and run (Windows):
 * <pre>
 * javac -cp jna.jar -d out $(find src/main/java -name '*.java') examples/Example.java
 * java -cp "out;jna.jar" -Djna.library.path=../../target/release Example
 * </pre>
 */
public final class Example {

    private static void check(boolean condition, String message) {
        if (!condition) {
            throw new AssertionError(message);
        }
    }

    public static void main(String[] args) {
        String version = IlinkSdk.version();
        check(version.contains("."), "unexpected version: " + version);
        System.out.println("version   : " + version);

        IlinkClient client = new IlinkClient();

        // Typed helpers decode results into the IlinkModels structs.
        MimeInfo info = client.mimeInfo("photo.PNG");
        check("image/png".equals(info.mime) && info.isImage, "unexpected mime: " + info.mime);
        System.out.println("mime.info : " + info.mime + " (image=" + info.isImage
                + ", ext=" + info.extension + ")");

        CryptoResult encrypted = client.cryptoEncrypt(
                "00112233445566778899aabbccddeeff", "hello world".getBytes());
        check(encrypted.paddedSize != null && encrypted.paddedSize == 16, "unexpected padding");
        System.out.println("encrypt   : " + encrypted.dataBase64);

        List<Account> accounts = client.accounts();
        System.out.println("accounts  : " + accounts.size());

        // The raw entry points remain available.
        JsonValue extension = IlinkSdk.callValue("mime.extension", "{\"mime\":\"image/png\"}");
        check(".png".equals(extension.get("extension").asString("")), "unexpected extension");
        System.out.println("extension : " + extension.get("extension").asString(""));

        try {
            IlinkSdk.call("does.not.exist", "{}");
            throw new AssertionError("expected IlinkError");
        } catch (IlinkError err) {
            check("unknown".equals(err.getKind()), "unexpected kind: " + err.getKind());
            System.out.println("error     : kind=" + err.getKind() + " message=" + err.getMessage());
        }

        System.out.println("all Java checks passed");
    }
}
