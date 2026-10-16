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
 * There are two ways in.
 *
 * 1. Typed functions (recommended). One C function per operation, with C
 *    structs for the results. No JSON to build or parse:
 *
 *        IlinkConfig config = {0};
 *        config.account_id = "cfe9a2017e77-im-bot";
 *
 *        IlinkMimeInfo info;
 *        if (ilink_mime_info("photo.png", &info) == ILINK_OK) {
 *            printf("%s %s\n", info.mime, info.extension);
 *            ilink_mime_info_free(&info);
 *        }
 *
 *        IlinkSendResult sent;
 *        if (ilink_send_text(&config, "u@im.wechat", "hi", NULL, NULL, &sent) == ILINK_OK) {
 *            printf("sent %s\n", sent.message_id);
 *            ilink_send_result_free(&sent);
 *        }
 *
 *    Conventions:
 *      - every function returns an IlinkStatus (ILINK_OK on success);
 *      - results are written through an out-pointer and released with the
 *        matching ilink_*_free() function, so the library frees its own memory;
 *        the free functions are NULL-safe and reset the struct;
 *      - optional string fields are NULL when absent;
 *      - `config` carries routing defaults (account id, base URL, timeout, ...).
 *        Pass NULL for the defaults. It is the C counterpart of the Client
 *        object the other bindings expose;
 *      - on failure, ilink_last_error() gives the message for the calling
 *        thread.
 *
 * 2. The universal JSON escape hatch. Every operation has a typed function
 *    below; `ilink_call` remains for forward compatibility (an operation added
 *    before its wrapper lands) or when you prefer JSON:
 *
 *        char *out = ilink_call("login.start", "{}");
 *        puts(out);   // {"ok":true,"result":{...}}
 *        ilink_string_free(out);
 *
 * Returned strings are heap-allocated and MUST be released with
 * ilink_string_free(). ilink_call never returns NULL for valid input.
 *
 * Threading: the library owns a lazily created Tokio runtime. Call the
 * functions from any thread that is not already running a Tokio runtime.
 *
 * Every operation below has a typed C function; the JSON escape hatch
 * (ilink_call) remains for anything added later. See docs/reference/ffi.md for
 * the full parameter and result schemas:
 *
 *   Meta
 *     version                       {}
 *     paths                         {}
 *   Accounts
 *     accounts.list                 {}
 *     account.resolve               {account_id}
 *     account.save                  {account_id, token?, ...}
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

#include <stddef.h>
#include <stdint.h>

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

/* ==========================================================================
 * Status codes
 * ========================================================================== */

/* ILINK_OK on success; anything else is a failure. */
typedef int IlinkStatus;

#define ILINK_OK               0
#define ILINK_ERR_INVALID_ARG  1  /* a NULL/invalid argument */
#define ILINK_ERR_NOT_FOUND    2  /* the resource does not exist */
#define ILINK_ERR_DNS          3
#define ILINK_ERR_TCP          4
#define ILINK_ERR_TLS          5
#define ILINK_ERR_TIMEOUT      6
#define ILINK_ERR_CANCELLED    7
#define ILINK_ERR_UNKNOWN      8
#define ILINK_ERR_PANIC        9  /* a bug in the SDK; please report it */

/* Human-readable text for a status code (static, never NULL). */
ILINK_API const char *ilink_status_message(IlinkStatus status);

/*
 * Message for the most recent failure on the calling thread. Valid until the
 * next ilink_* call on that thread. Never NULL; empty when there is no error.
 */
ILINK_API const char *ilink_last_error(void);

/* ==========================================================================
 * Result types
 *
 * Release each with its matching ilink_*_free() function. Those are NULL-safe
 * and reset the struct, so double-free is safe. Optional string fields are
 * NULL when absent.
 * ========================================================================== */

/* Routing defaults applied to every call. All fields are optional: leave them
 * zeroed/NULL for the built-in defaults. The C counterpart of the Client
 * object the other bindings expose. */
typedef struct IlinkConfig {
    const char *account_id;   /* which stored account to act as */
    const char *base_url;     /* override the API base URL */
    const char *cdn_base_url; /* override the CDN base URL */
    const char *route_tag;    /* observability routing tag */
    const char *bot_agent;    /* observability agent name */
    uint64_t    timeout_ms;   /* request timeout; 0 = default */
} IlinkConfig;

typedef struct IlinkPaths {
    char *state_dir;
    char *sdk_state_dir;
    char *accounts_dir;
    char *config_path;
} IlinkPaths;

typedef struct IlinkMimeInfo {
    char *mime;       /* e.g. "image/png" */
    char *kind;       /* "Image", "Video", "Audio" or "File" */
    char *extension;  /* e.g. ".png" (empty when unknown) */
    int   is_image;
    int   is_video;
    int   is_audio;
} IlinkMimeInfo;

typedef struct IlinkAccount {
    char *account_id;
    char *base_url;
    char *cdn_base_url;
    char *user_id;     /* NULL when unknown */
    char *name;        /* NULL when unnamed */
    char *route_tag;   /* NULL when unset */
    int   configured;
    int   enabled;
    int   token_present;
} IlinkAccount;

typedef struct IlinkAccountList {
    IlinkAccount *accounts;  /* NULL when count is 0 */
    size_t        count;
} IlinkAccountList;

typedef struct IlinkSendResult {
    char *message_id;
    char *server_message_id;
} IlinkSendResult;

typedef struct IlinkSessionStatus {
    int   paused;
    int64_t remaining_ms;
} IlinkSessionStatus;

typedef struct IlinkCryptoResult {
    char  *data_base64;
    int64_t padded_size;  /* -1 when not reported (crypto.decrypt) */
} IlinkCryptoResult;

typedef struct IlinkUploadedFile {
    char   *filekey;
    char   *download_param;
    char   *aeskey_hex;
    char   *aes_key_base64;
    uint64_t file_size;
    uint64_t file_size_ciphertext;
} IlinkUploadedFile;

typedef struct IlinkMediaFetch {
    int64_t bytes;
    char   *path;         /* NULL when returned inline */
    char   *data_base64;  /* NULL when written to dest_path */
} IlinkMediaFetch;

typedef struct IlinkGetConfig {
    int64_t ret;
    char   *errmsg;        /* NULL when the server sent none */
    char   *typing_ticket; /* NULL when the server sent none */
    int     ok;            /* non-zero when ret == 0 */
} IlinkGetConfig;

/* One inbound message. `items_json` is the raw `item_list` array: pass it to
 * ilink_message_body() for the plain text, or decode it yourself. */
typedef struct IlinkMessage {
    int64_t seq;
    char   *message_id;
    char   *from_user_id;
    char   *to_user_id;
    char   *session_id;
    int64_t create_time_ms;
    char   *context_token;
    char   *run_id;
    char   *items_json;
} IlinkMessage;

typedef struct IlinkUpdates {
    int64_t ret;
    int64_t errcode;
    char   *errmsg;
    char   *get_updates_buf;  /* feed back into ilink_get_updates() */
    int64_t longpolling_timeout_ms;
    IlinkMessage *messages;   /* NULL when count is 0 */
    size_t        count;
    int           is_error;   /* non-zero when the server reported a failure */
} IlinkUpdates;

/* -- QR login ------------------------------------------------------------- */

typedef struct IlinkQrSession {
    char *token;     /* bot token for authenticated requests */
    char *bot_id;    /* bot account id */
    char *base_url;  /* API base URL assigned to the account */
    char *user_id;   /* the Weixin user id that scanned */
} IlinkQrSession;

/* login.start / login.refresh */
typedef struct IlinkLoginStart {
    char *qrcode;       /* the string to render as a QR code */
    char *qrcode_url;   /* a URL that renders the QR code */
    char *session_key;  /* pass this to the other login.* calls */
} IlinkLoginStart;

/* login.poll: `kind` is one of "continue", "needs_verification", "expired",
 * "verify_code_blocked", "already_connected", "done". */
typedef struct IlinkLoginPoll {
    char *kind;
    int   retry;                /* set for "needs_verification" */
    IlinkQrSession *session;    /* set for "done"; release with ilink_login_poll_free */
} IlinkLoginPoll;

/* login.wait / login.complete */
typedef struct IlinkLoginResult {
    int   connected;
    int   already_connected;    /* already paired; nothing new to persist */
    char *message;              /* explanation when connected == 0 */
    char *account_id;           /* set by login.complete on success; else NULL */
    IlinkQrSession *session;    /* NULL when the login did not complete */
} IlinkLoginResult;

/* -- inbound message context --------------------------------------------- */

typedef struct IlinkMediaEntry {
    char *path;
    char *content_type;  /* NULL when unknown */
    char *file_name;     /* NULL when unknown */
    char *message_id;    /* NULL when unknown */
} IlinkMediaEntry;

/* The decoded form of an inbound message. */
typedef struct IlinkMsgContext {
    char *body;
    char *from;
    char *to;
    char *account_id;
    char *originating_channel;
    char *originating_to;
    char *message_sid;
    char *message_sid_full;      /* NULL when absent */
    int64_t timestamp;           /* 0 when absent */
    char *provider;
    char *chat_type;
    char *session_key;           /* NULL when absent */
    char *context_token;         /* NULL when absent */
    char *media_path;            /* NULL when absent */
    char *media_type;            /* NULL when absent */
    char **media_paths;
    size_t media_paths_count;
    char **media_types;
    size_t media_types_count;
    IlinkMediaEntry *media;
    size_t media_count;
    char **channel_prompt_context;
    size_t channel_prompt_context_count;
    char *reply_to_id;           /* NULL when absent */
    char *reply_to_body;         /* NULL when absent */
    char *reply_to_quote_text;   /* NULL when absent */
    int   reply_to_is_quote;     /* non-zero when set and true */
    int   reply_to_is_quote_set; /* non-zero when the field was present */
} IlinkMsgContext;

/* -- media ---------------------------------------------------------------- */

typedef struct IlinkInboundMedia {
    char *image_path;  /* NULL when no image was downloaded */
    char *voice_path;
    char *voice_mime;
    char *file_path;
    char *file_mime;
    char *video_path;
    int   referenced;  /* came from an inline quoted message */
    int   is_empty;
} IlinkInboundMedia;

/* -- quote cache ---------------------------------------------------------- */

typedef struct IlinkQuotePolicy {
    int      enabled;
    uint64_t retention_ms;
    uint64_t max_messages_per_account;
    uint64_t media_retention_ms;
    uint64_t max_media_bytes_per_account;
    uint64_t max_single_media_bytes;
} IlinkQuotePolicy;

typedef struct IlinkQuoteRecord {
    char   *account_id;
    char   *conversation_id;
    char   *message_id;
    char   *direction;      /* "inbound" or "outbound" */
    char   *body;
    char   *media_path;     /* NULL when absent */
    char   *media_mime;     /* NULL when absent */
    char   *media_name;     /* NULL when absent */
    int64_t media_size;     /* 0 when absent */
    int64_t created_at;
} IlinkQuoteRecord;

/* One `quote.put` entry. */
typedef struct IlinkQuoteInput {
    const char *account_id;
    const char *conversation_id;
    const char *message_id;
    const char *direction;        /* "inbound" (default) or "outbound" */
    const char *body;
    const char *source_media_path;/* NULL to skip */
    const char *media_mime;
    const char *media_name;
    int64_t     created_at;       /* 0 = now */
} IlinkQuoteInput;

/* -- config --------------------------------------------------------------- */

/* The local SDK config, normalized. `accounts_json` is the per-account
 * override map as JSON text (it is free-form, so it is not typed). */
typedef struct IlinkLocalConfig {
    int   reply_progress_messages;
    char *accounts_json;
    char *route_tag_json;          /* NULL when unset */
    char *bot_agent;               /* NULL when unset */
    char *channel_config_updated_at;/* NULL when unset */
    IlinkQuotePolicy quote_cache_policy;
} IlinkLocalConfig;

/* ==========================================================================
 * Meta
 * ========================================================================== */

/* SDK version (static, never NULL). */
ILINK_API const char *ilink_version_string(void);

ILINK_API IlinkStatus ilink_paths(const IlinkConfig *config, IlinkPaths *out);
ILINK_API void        ilink_paths_free(IlinkPaths *paths);

/* ==========================================================================
 * Pure helpers (no network, no account needed)
 * ========================================================================== */

ILINK_API IlinkStatus ilink_mime_info(const char *path, IlinkMimeInfo *out);
ILINK_API void        ilink_mime_info_free(IlinkMimeInfo *info);

/* `*out` is a newly allocated string (release with ilink_string_free). */
ILINK_API IlinkStatus ilink_mime_extension(const char *mime, char **out);
ILINK_API IlinkStatus ilink_generate_id(const char *prefix, char **out);
ILINK_API IlinkStatus ilink_file_key(char **out);
ILINK_API IlinkStatus ilink_temp_name(const char *prefix, const char *ext, char **out);
ILINK_API IlinkStatus ilink_cdn_download_url(const char *encrypted_query_param,
                                             const char *cdn_base_url, char **out);
ILINK_API IlinkStatus ilink_cdn_upload_url(const char *upload_param,
                                           const char *filekey,
                                           const char *cdn_base_url, char **out);

/* AES-128-ECB. `data`/`len` is the raw input; the result is base64. */
ILINK_API IlinkStatus ilink_crypto_encrypt(const char *key_hex,
                                           const uint8_t *data, size_t len,
                                           IlinkCryptoResult *out);
ILINK_API IlinkStatus ilink_crypto_decrypt(const char *key_hex,
                                           const uint8_t *data, size_t len,
                                           IlinkCryptoResult *out);
ILINK_API void        ilink_crypto_result_free(IlinkCryptoResult *result);

/* ==========================================================================
 * Accounts
 * ========================================================================== */

ILINK_API IlinkStatus ilink_accounts_list(const IlinkConfig *config,
                                          IlinkAccountList *out);
ILINK_API void        ilink_account_list_free(IlinkAccountList *list);
ILINK_API void        ilink_account_free(IlinkAccount *account);

ILINK_API IlinkStatus ilink_account_resolve(const char *account_id,
                                            IlinkAccount *out);

/* token/base_url/user_id may each be NULL to leave them unchanged. */
ILINK_API IlinkStatus ilink_account_save(const char *account_id,
                                         const char *token,
                                         const char *base_url,
                                         const char *user_id,
                                         IlinkAccount *out);
ILINK_API IlinkStatus ilink_account_clear(const char *account_id);

/* ==========================================================================
 * Session guard
 * ========================================================================== */

ILINK_API IlinkStatus ilink_session_status(const char *account_id,
                                           IlinkSessionStatus *out);
ILINK_API IlinkStatus ilink_session_pause(const char *account_id);
ILINK_API IlinkStatus ilink_session_resume(const char *account_id);
/* Fails when the session is paused. */
ILINK_API IlinkStatus ilink_session_assert(const char *account_id);

/* ==========================================================================
 * QR login
 * ========================================================================== */

/* Start a login: returns the QR code to display and the session key. */
ILINK_API IlinkStatus ilink_login_start(const IlinkConfig *config,
                                        const char *base_url,
                                        const char *bot_type,
                                        const char *session_key,
                                        IlinkLoginStart *out);
ILINK_API void ilink_login_start_free(IlinkLoginStart *start);

/* Re-render an expired QR code. */
ILINK_API IlinkStatus ilink_login_refresh(const IlinkConfig *config,
                                          const char *session_key,
                                          IlinkLoginStart *out);

/* Poll once. Check `out->kind` to decide what to do next. */
ILINK_API IlinkStatus ilink_login_poll(const IlinkConfig *config,
                                       const char *session_key,
                                       IlinkLoginPoll *out);
ILINK_API void ilink_login_poll_free(IlinkLoginPoll *poll);

/* Submit the verification code shown on the phone. */
ILINK_API IlinkStatus ilink_login_submit_verify_code(const IlinkConfig *config,
                                                     const char *session_key,
                                                     const char *code);

/* Block until the login finishes (or the timeout expires). `verify_code` may be
 * NULL. `timeout_secs` of 0 means the default (480). */
ILINK_API IlinkStatus ilink_login_wait(const IlinkConfig *config,
                                       const char *session_key,
                                       uint64_t timeout_secs,
                                       const char *verify_code,
                                       IlinkLoginResult *out);

/* Like ilink_login_wait, and on success persists the credentials. `out->account_id`
 * is then the account to use for later calls. */
ILINK_API IlinkStatus ilink_login_complete(const IlinkConfig *config,
                                           const char *session_key,
                                           uint64_t timeout_secs,
                                           const char *verify_code,
                                           IlinkLoginResult *out);
ILINK_API void ilink_login_result_free(IlinkLoginResult *result);
ILINK_API void ilink_qr_session_free(IlinkQrSession *session);

/* ==========================================================================
 * Sending
 * ========================================================================== */

ILINK_API void ilink_send_result_free(IlinkSendResult *result);

/* context_token and run_id may be NULL. */
ILINK_API IlinkStatus ilink_send_text(const IlinkConfig *config,
                                      const char *to,
                                      const char *text,
                                      const char *context_token,
                                      const char *run_id,
                                      IlinkSendResult *out);

ILINK_API IlinkStatus ilink_send_file(const IlinkConfig *config,
                                      const char *to,
                                      const char *path,
                                      const char *caption,
                                      const char *context_token,
                                      IlinkSendResult *out);

/* Send in-memory bytes; `file_name` is what the recipient sees. */
ILINK_API IlinkStatus ilink_send_bytes(const IlinkConfig *config,
                                       const char *to,
                                       const char *file_name,
                                       const uint8_t *data, size_t len,
                                       const char *caption,
                                       IlinkSendResult *out);

/* Send a pre-built message item. `item_json` is one `MessageItem` object in the
 * wire shape (the same shape found in `IlinkMessage.items_json`). The nested
 * item union is too irregular to model as a C struct, so it is passed as JSON
 * text here; everything else about the call is typed. */
ILINK_API IlinkStatus ilink_send_item(const IlinkConfig *config,
                                      const char *to,
                                      const char *item_json,
                                      const char *context_token,
                                      IlinkSendResult *out);

/* ==========================================================================
 * Receiving
 * ========================================================================== */

/* Long-poll for new messages. Pass the previous `get_updates_buf` (or NULL on
 * the first call) and feed the returned one back in next time. */
ILINK_API IlinkStatus ilink_get_updates(const IlinkConfig *config,
                                        const char *get_updates_buf,
                                        uint64_t timeout_ms,
                                        IlinkUpdates *out);
ILINK_API void        ilink_updates_free(IlinkUpdates *updates);

/* Extract the plain-text body from `IlinkMessage.items_json`. */
ILINK_API IlinkStatus ilink_message_body(const char *items_json, char **out);

/* Decode a raw inbound message (e.g. one entry of `IlinkUpdates.messages`,
 * serialized as JSON) into a context. No media is downloaded. `items_json` and
 * `context_token` may be NULL. */
ILINK_API IlinkStatus ilink_context_to_message(const IlinkConfig *config,
                                               const char *message_json,
                                               IlinkMsgContext *out);
ILINK_API void ilink_msg_context_free(IlinkMsgContext *context);

ILINK_API IlinkStatus ilink_get_config(const IlinkConfig *config,
                                       const char *user_id,
                                       const char *context_token,
                                       IlinkGetConfig *out);
ILINK_API void        ilink_get_config_free(IlinkGetConfig *config);

/* status: 1 = typing, 0 = stopped. */
ILINK_API IlinkStatus ilink_send_typing(const IlinkConfig *config,
                                        const char *user_id,
                                        const char *typing_ticket,
                                        int status);

/* start: non-zero to announce the bot is online, zero for offline. */
ILINK_API IlinkStatus ilink_notify(int start);

/* ==========================================================================
 * Media
 * ========================================================================== */

ILINK_API void ilink_media_fetch_free(IlinkMediaFetch *fetch);
ILINK_API void ilink_uploaded_file_free(IlinkUploadedFile *file);

/* Download an arbitrary URL. With dest_path the bytes go to that file and
 * `out->path` is set; otherwise they come back base64 in `out->data_base64`. */
ILINK_API IlinkStatus ilink_fetch(const IlinkConfig *config,
                                  const char *url,
                                  const char *dest_path,
                                  IlinkMediaFetch *out);

/* Download an encrypted CDN media reference. Give either the encrypted query
 * parameter or a full URL. `aes_key` is the base64/hex key, or NULL for
 * unencrypted media. */
ILINK_API IlinkStatus ilink_download(const IlinkConfig *config,
                                     const char *encrypted_query_param,
                                     const char *aes_key,
                                     const char *full_url,
                                     const char *dest_path,
                                     IlinkMediaFetch *out);

/* Upload a local file. media_type is "image", "video", "file" or "voice",
 * or NULL to infer it from the extension. */
ILINK_API IlinkStatus ilink_upload(const IlinkConfig *config,
                                   const char *to,
                                   const char *path,
                                   const char *media_type,
                                   IlinkUploadedFile *out);

/* Download the media carried by an inbound message item. `item_json` is one
 * entry of `IlinkMessage.items_json`. */
ILINK_API IlinkStatus ilink_download_item(const IlinkConfig *config,
                                          const char *item_json,
                                          const char *dest_dir,
                                          IlinkInboundMedia *out);
ILINK_API void ilink_inbound_media_free(IlinkInboundMedia *media);

/* ==========================================================================
 * Quote cache
 * ========================================================================== */

ILINK_API IlinkStatus ilink_quote_policy(IlinkQuotePolicy *out);

/* `*found` is non-zero when a record was returned. */
ILINK_API IlinkStatus ilink_quote_find(const char *account_id,
                                       const char *conversation_id,
                                       const char *message_id,
                                       IlinkQuoteRecord *out,
                                       int *found);
ILINK_API void ilink_quote_record_free(IlinkQuoteRecord *record);

ILINK_API IlinkStatus ilink_quote_put(const IlinkQuoteInput *input);
ILINK_API IlinkStatus ilink_quote_gc(void);
ILINK_API IlinkStatus ilink_quote_delete_account(const char *account_id);
/* `*out` is the managed media directory for the account. */
ILINK_API IlinkStatus ilink_quote_media_dir(const char *account_id, char **out);

/* ==========================================================================
 * Local config
 * ========================================================================== */

ILINK_API IlinkStatus ilink_config_load(IlinkLocalConfig *out);
ILINK_API void        ilink_local_config_free(IlinkLocalConfig *config);

/* ==========================================================================
 * Context tokens / sync cursor
 * ========================================================================== */

/* `*out` is the stored token, or an empty string when none is stored. */
ILINK_API IlinkStatus ilink_context_get(const IlinkConfig *config,
                                        const char *user_id, char **out);
ILINK_API IlinkStatus ilink_context_set(const IlinkConfig *config,
                                        const char *user_id,
                                        const char *token);
ILINK_API IlinkStatus ilink_context_clear(const IlinkConfig *config);
ILINK_API IlinkStatus ilink_context_restore(const IlinkConfig *config);

ILINK_API IlinkStatus ilink_sync_load(const IlinkConfig *config, char **out);
ILINK_API IlinkStatus ilink_sync_save(const IlinkConfig *config, const char *buf);
ILINK_API IlinkStatus ilink_sync_clear(const IlinkConfig *config);

/* ==========================================================================
 * Universal JSON escape hatch
 * ========================================================================== */

/*
 * Run any operation, including ones added before their typed wrapper lands.
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
