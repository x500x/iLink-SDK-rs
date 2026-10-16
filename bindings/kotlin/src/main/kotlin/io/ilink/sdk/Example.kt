package io.ilink.sdk

/**
 * Minimal Kotlin consumer / smoke test of the iLink SDK.
 *
 * Build the library first (repository root): `cargo build --release`
 * Then: `./gradlew run -Djna.library.path=../../target/release`
 */
fun main() {
    val version = IlinkSdk.version()
    check(version.contains(".")) { "unexpected version: $version" }
    println("version   : $version")

    val mime = IlinkSdk.call("mime.info", """{"path":"photo.PNG"}""")
    check(mime.contains("\"mime\":\"image/png\"")) { "unexpected mime: $mime" }
    println("mime.info : $mime")

    val encrypted = IlinkSdk.call(
        "crypto.encrypt",
        """{"key_hex":"00112233445566778899aabbccddeeff","data_base64":"aGVsbG8gd29ybGQ="}""",
    )
    check(encrypted.contains("padded_size")) { "unexpected encrypt: $encrypted" }
    println("encrypt   : $encrypted")

    val client = IlinkClient()
    println("accounts  : ${client.accounts()}")

    try {
        IlinkSdk.call("does.not.exist", "{}")
        error("expected IlinkError")
    } catch (err: IlinkError) {
        check(err.kind == "unknown") { "unexpected kind: ${err.kind}" }
        println("error     : kind=${err.kind} message=${err.message}")
    }

    println("all Kotlin checks passed")
}
