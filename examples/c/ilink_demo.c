/*
 * ilink_demo.c — minimal C consumer of the iLink SDK shared library.
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
 */

#include <stdio.h>
#include "ilink_sdk.h"

static void show(const char *op, const char *params) {
    char *out = ilink_call(op, params);
    printf("%-22s -> %s\n", op, out ? out : "(null)");
    ilink_string_free(out);
}

int main(void) {
    char *version = ilink_version();
    printf("version                -> %s\n", version);
    ilink_string_free(version);

    /* Pure helpers need no account or network. */
    show("mime.info", "{\"path\":\"photo.PNG\"}");
    show("cdn.download_url", "{\"encrypted_query_param\":\"a+b/c\"}");
    show("crypto.encrypt",
         "{\"key_hex\":\"00112233445566778899aabbccddeeff\","
         "\"data_base64\":\"aGVsbG8gd29ybGQ=\"}");

    /* Account operations read the same state directory as the Rust CLI. */
    show("accounts.list", "{}");
    show("session.status", "{\"account_id\":\"demo\"}");

    /* Unknown operations return {"ok":false,...} rather than crashing. */
    show("does.not.exist", "{}");

    return 0;
}
