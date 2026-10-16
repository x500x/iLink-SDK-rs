package io.ilink.sdk;

/**
 * An iLink SDK operation failed. {@link #getKind()} mirrors {@code ilink_sdk::ErrorKind}
 * ({@code dns}, {@code tcp}, {@code tls}, {@code timeout}, {@code cancelled}, ...).
 */
public class IlinkError extends RuntimeException {

    private final String kind;

    public IlinkError(String kind, String message) {
        super(message);
        this.kind = kind == null ? "unknown" : kind;
    }

    public String getKind() {
        return kind;
    }
}
