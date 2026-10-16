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

    val client = IlinkClient()

    // Typed helpers decode results into the IlinkModels data classes.
    val info = client.mimeInfo("photo.PNG")
    check(info.mime == "image/png" && info.isImage) { "unexpected mime: ${info.mime}" }
    println("mime.info : ${info.mime} (image=${info.isImage}, ext=${info.extension})")

    val encrypted = client.cryptoEncrypt("00112233445566778899aabbccddeeff", "hello world".toByteArray())
    check(encrypted.paddedSize == 16L) { "unexpected padding: ${encrypted.paddedSize}" }
    println("encrypt   : ${encrypted.dataBase64}")

    val accounts = client.accounts()
    println("accounts  : ${accounts.size}")

    // The raw entry points remain available.
    val extension = IlinkSdk.callValue("mime.extension", """{"mime":"image/png"}""")
    check(extension["extension"].asString() == ".png") { "unexpected extension" }
    println("extension : ${extension["extension"].asString()}")

    try {
        IlinkSdk.call("does.not.exist", "{}")
        error("expected IlinkError")
    } catch (err: IlinkError) {
        check(err.kind == "unknown") { "unexpected kind: ${err.kind}" }
        println("error     : kind=${err.kind} message=${err.message}")
    }

    println("all Kotlin checks passed")
}
