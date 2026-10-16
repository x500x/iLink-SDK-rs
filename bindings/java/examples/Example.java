import io.ilink.sdk.IlinkClient;
import io.ilink.sdk.IlinkError;
import io.ilink.sdk.IlinkSdk;

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

        String mime = IlinkSdk.call("mime.info", "{\"path\":\"photo.PNG\"}");
        check(mime.contains("\"mime\":\"image/png\""), "unexpected mime: " + mime);
        System.out.println("mime.info : " + mime);

        String encrypted = IlinkSdk.call("crypto.encrypt",
                "{\"key_hex\":\"00112233445566778899aabbccddeeff\",\"data_base64\":\"aGVsbG8gd29ybGQ=\"}");
        check(encrypted.contains("padded_size"), "unexpected encrypt: " + encrypted);
        System.out.println("encrypt   : " + encrypted);

        IlinkClient client = new IlinkClient();
        System.out.println("accounts  : " + client.accounts());

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
