/*
 * ilink_sdk.h — C ABI for the iLink (Weixin) SDK.
 *
 * Build the library with `cargo build --release`, which produces:
 *   Windows : ilink_sdk.dll  + ilink_sdk.dll.lib (import) + ilink_sdk.lib (static)
 *   Linux   : libilink_sdk.so + libilink_sdk.a
 *   macOS   : libilink_sdk.dylib + libilink_sdk.a
 *
 * Link against the shared library (dll/so/dylib) or the static library
 * (lib/.a). The Rust rlib is also produced for pure-Rust consumers.
 *
 * Linking:
 *   - Shared library (recommended): link `ilink_sdk.dll.lib` (MSVC) or the
 *     `.so`/`.dylib` directly, and ship the runtime library next to your
 *     program. Nothing extra needs to be defined.
 *   - Static library: define `ILINK_SDK_STATIC` before including this header,
 *     otherwise the declarations are marked `dllimport` and the linker looks
 *     for `__imp_ilink_call` instead of `ilink_call`. On Windows the static
 *     library is built with the MSVC toolchain and can only be linked by
 *     MSVC (it needs `__chkstk` and the MSVC C++ type-info runtime); use the
 *     shared library from MinGW/Go cgo instead.
 *
 * Everything is reached through a single JSON-in / JSON-out call:
 *
 *     char *out = ilink_call("send.text", "{\"to\":\"u@im.wechat\",\"text\":\"hi\"}");
 *     if (out) {
 *         // out is a JSON object: {"ok":true,"result":...} or {"ok":false,"error":...}
 *         puts(out);
 *         ilink_string_free(out);
 *     }
 *
 * Returned strings are heap-allocated and MUST be released with
 * ilink_string_free(). ilink_call never returns NULL for valid input.
 *
 * Threading: the library owns a lazily created Tokio runtime. Call the
 * functions from any thread that is not already running a Tokio runtime.
 *
 * Operations (the `op` string) — see docs/reference/ffi.md for the full
 * parameter and result schemas:
 *
 *   Meta
 *     version                       {}
 *     paths                         {}
 *   Accounts
 *     accounts.list                 {}
 *     account.resolve               {account_id}
 *     account.save                  {account_id, token?, base_url?, user_id?}
 *     account.clear                 {account_id}
 *   QR login
 *     login.start                   {base_url?, bot_type?, session_key?}
 *     login.poll                    {session_key, base_url?}
 *     login.submit_verify_code      {session_key, code}
 *     login.refresh                 {session_key}
 *     login.wait                    {session_key, timeout_secs?, verify_code?}
 *     login.complete                {session_key, timeout_secs?, verify_code?}
 *   Outbound
 *     send.text                     {to, text, context_token?, run_id?, account_id?}
 *     send.file                     {to, path, caption?, context_token?, account_id?}
 *     send.item                     {to, item, context_token?, account_id?}
 *     send.bytes                    {to, file_name, data_base64, caption?, account_id?}
 *   Inbound / API
 *     get_updates                   {get_updates_buf?, timeout_ms?, account_id?}
 *     get_config                    {user_id, context_token?, account_id?}
 *     send_typing                   {user_id, typing_ticket, status?, account_id?}
 *     notify                        {action:"start"|"stop", account_id?}
 *     context.to_message            {message, account_id?}
 *     message.body                  {items}
 *   Media
 *     media.fetch                   {url, dest_path?}
 *     media.download                {media, aes_key?, cdn_base_url?, dest_path?}
 *     media.download_item           {item, cdn_base_url?, dest_dir?}
 *     media.upload                  {to, path, media_type?, cdn_base_url?, account_id?}
 *   Quote cache
 *     quote.policy                  {}
 *     quote.find                    {account_id, conversation_id, message_id}
 *     quote.put                     {account_id, conversation_id, message_id, body?, ...}
 *     quote.gc                      {}
 *     quote.delete_account          {account_id}
 *     quote.media_dir               {account_id}
 *   Context tokens
 *     context.get                   {account_id, user_id}
 *     context.set                   {account_id, user_id, token}
 *     context.clear                 {account_id}
 *     context.restore               {account_id}
 *   Sync cursor
 *     sync.load                     {account_id}
 *     sync.save                     {account_id, buf}
 *     sync.clear                    {account_id}
 *   Session guard
 *     session.status                {account_id}
 *     session.pause                 {account_id}
 *     session.resume                {account_id}
 *     session.assert                {account_id}
 *   Helpers
 *     mime.info                     {path}
 *     mime.extension                {mime}
 *     config.load                   {}
 *     util.generate_id              {prefix?}
 *     util.file_key                 {}
 *     util.temp_name                {prefix?, ext?}
 *     cdn.download_url              {encrypted_query_param, cdn_base_url?}
 *     cdn.upload_url                {upload_param, filekey, cdn_base_url?}
 *     crypto.encrypt                {key_hex, data_base64}
 *     crypto.decrypt                {key_hex, data_base64}
 */

#ifndef ILINK_SDK_H
#define ILINK_SDK_H

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32)
#  if defined(ILINK_SDK_STATIC)
     /* Static library: the symbols live in the consumer's binary. */
#    define ILINK_API
#  elif defined(ILINK_SDK_BUILD)
     /* Building the DLL itself. */
#    define ILINK_API __declspec(dllexport)
#  else
     /* Default: linking the import library / the DLL. */
#    define ILINK_API __declspec(dllimport)
#  endif
#else
#  define ILINK_API __attribute__((visibility("default")))
#endif

/*
 * Run one SDK operation.
 *
 * op          NUL-terminated operation name (see the list above).
 * params_json NUL-terminated JSON object, or NULL for no parameters.
 *
 * Returns a newly allocated NUL-terminated JSON string:
 *   {"ok":true,  "result": <operation result>}
 *   {"ok":false, "error": "<message>", "kind": "<error kind>"}
 * The caller must release it with ilink_string_free().
 */
ILINK_API char *ilink_call(const char *op, const char *params_json);

/* Library version as a newly allocated string (release with ilink_string_free). */
ILINK_API char *ilink_version(void);

/* Free a string previously returned by ilink_call() or ilink_version(). */
ILINK_API void ilink_string_free(char *ptr);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* ILINK_SDK_H */
