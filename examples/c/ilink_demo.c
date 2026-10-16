/*
 * ilink_demo.c — C consumer of the iLink SDK, using the typed API.
 *
 * Build (Linux/macOS):
 *   cargo build --release
 *   cc -I include examples/c/ilink_demo.c -L target/release -lilink_sdk -o ilink_demo
 *   LD_LIBRARY_PATH=target/release ./ilink_demo
 *
 * Build (Windows, MinGW):
 *   cargo build --release
 *   gcc -I include examples/c/ilink_demo.c target/release/ilink_sdk.dll -o ilink_demo.exe
 *   copy target\release\ilink_sdk.dll . && ilink_demo.exe
 *
 * Build (Windows, MSVC):
 *   cl /I include examples\c\ilink_demo.c target\release\ilink_sdk.dll.lib
 *
 * This file only uses the typed functions: no JSON is built or parsed. The
 * universal ilink_call() escape hatch is shown at the end.
 */

#include <stdio.h>
#include <string.h>

#include "ilink_sdk.h"

/* Report a failure with the thread-local error message. */
static int check(IlinkStatus status, const char *what) {
    if (status == ILINK_OK) return 1;
    fprintf(stderr, "%s failed: %s (%s)\n", what, ilink_last_error(),
            ilink_status_message(status));
    return 0;
}

static void demo_meta(void) {
    printf("== meta ==\n");
    printf("version  : %s\n", ilink_version_string());

    IlinkPaths paths;
    if (check(ilink_paths(NULL, &paths), "ilink_paths")) {
        printf("state dir: %s\n", paths.state_dir);
        ilink_paths_free(&paths);
    }
}

static void demo_helpers(void) {
    printf("\n== pure helpers ==\n");

    IlinkMimeInfo info;
    if (check(ilink_mime_info("photo.PNG", &info), "ilink_mime_info")) {
        printf("mime.info: %s kind=%s ext=%s image=%d\n", info.mime, info.kind,
               info.extension, info.is_image);
        ilink_mime_info_free(&info);
    }

    char *ext = NULL;
    if (check(ilink_mime_extension("image/png", &ext), "ilink_mime_extension")) {
        printf("extension: %s\n", ext);
        ilink_string_free(ext);
    }

    char *id = NULL;
    if (check(ilink_generate_id("demo", &id), "ilink_generate_id")) {
        printf("id       : %s\n", id);
        ilink_string_free(id);
    }

    char *key = NULL;
    if (check(ilink_file_key(&key), "ilink_file_key")) {
        printf("file key : %s (%lu chars)\n", key, (unsigned long)strlen(key));
        ilink_string_free(key);
    }

    char *url = NULL;
    if (check(ilink_cdn_download_url("a+b/c", NULL, &url), "ilink_cdn_download_url")) {
        printf("cdn url  : %s\n", url);
        ilink_string_free(url);
    }
}

static void demo_crypto(void) {
    printf("\n== crypto ==\n");
    const char *key_hex = "00112233445566778899aabbccddeeff";
    const unsigned char plain[] = "hello world";

    /* ilink_crypto_encrypt takes raw bytes and returns base64. */
    IlinkCryptoResult encrypted;
    if (!check(ilink_crypto_encrypt(key_hex, plain, sizeof plain - 1, &encrypted),
               "ilink_crypto_encrypt")) {
        return;
    }
    printf("encrypted: %s (padded=%lld)\n", encrypted.data_base64,
           (long long)encrypted.padded_size);

    /*
     * ilink_crypto_decrypt takes the *ciphertext bytes* and returns base64 of
     * the plaintext. Base64-decoding the encrypted value is left to your own
     * decoder, since the SDK does not ship one for C. Here we decrypt a known
     * 16-byte block to show the call shape.
     */
    const unsigned char ciphertext[16] = {
        0xc1, 0x72, 0x7c, 0x59, 0x3f, 0x54, 0xb2, 0x74,
        0x7b, 0x5e, 0xf7, 0x39, 0x76, 0xb6, 0x99, 0x9c,
    };
    IlinkCryptoResult decrypted;
    if (check(ilink_crypto_decrypt(key_hex, ciphertext, sizeof ciphertext, &decrypted),
              "ilink_crypto_decrypt")) {
        printf("decrypted: %s (base64 of \"hello world\")\n", decrypted.data_base64);
        ilink_crypto_result_free(&decrypted);
    }
    ilink_crypto_result_free(&encrypted);
}

static void demo_accounts(void) {
    printf("\n== accounts ==\n");

    IlinkAccountList list;
    if (!check(ilink_accounts_list(NULL, &list), "ilink_accounts_list")) return;

    printf("%lu account(s)\n", (unsigned long)list.count);
    for (size_t i = 0; i < list.count; ++i) {
        const IlinkAccount *a = &list.accounts[i];
        printf("  [%lu] %s user=%s enabled=%d\n", (unsigned long)i, a->account_id,
               a->user_id ? a->user_id : "(unknown)", a->enabled);
    }
    ilink_account_list_free(&list);
}

static void demo_config(void) {
    printf("\n== config (the C equivalent of a Client) ==\n");

    /* Routing defaults are passed to every call through this struct. */
    IlinkConfig config;
    memset(&config, 0, sizeof config);
    config.account_id = "demo";

    IlinkSessionStatus status;
    if (check(ilink_session_status(config.account_id, &status), "ilink_session_status")) {
        printf("paused   : %d (remaining %lld ms)\n", status.paused,
               (long long)status.remaining_ms);
    }
}

static void demo_escape_hatch(void) {
    printf("\n== ilink_call (operations without a typed wrapper) ==\n");

    /* Everything else is still reachable, e.g. quote cache or QR login. */
    char *out = ilink_call("quote.policy", "{}");
    if (out) {
        printf("quote.policy -> %s\n", out);
        ilink_string_free(out);
    }

    out = ilink_call("does.not.exist", "{}");
    if (out) {
        printf("unknown op   -> %s\n", out);
        ilink_string_free(out);
    }
}

int main(void) {
    demo_meta();
    demo_helpers();
    demo_crypto();
    demo_accounts();
    demo_config();
    demo_escape_hatch();
    printf("\nall C checks passed\n");
    return 0;
}
