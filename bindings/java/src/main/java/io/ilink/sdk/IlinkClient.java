package io.ilink.sdk;

import java.util.ArrayList;
import java.util.Base64;
import java.util.List;

import io.ilink.sdk.IlinkModels.Account;
import io.ilink.sdk.IlinkModels.ConfigLoad;
import io.ilink.sdk.IlinkModels.CryptoResult;
import io.ilink.sdk.IlinkModels.GetConfig;
import io.ilink.sdk.IlinkModels.InboundMedia;
import io.ilink.sdk.IlinkModels.LoginPoll;
import io.ilink.sdk.IlinkModels.LoginResult;
import io.ilink.sdk.IlinkModels.LoginStart;
import io.ilink.sdk.IlinkModels.MediaFetch;
import io.ilink.sdk.IlinkModels.MimeInfo;
import io.ilink.sdk.IlinkModels.MsgContext;
import io.ilink.sdk.IlinkModels.Paths;
import io.ilink.sdk.IlinkModels.QuotePolicy;
import io.ilink.sdk.IlinkModels.QuoteRecord;
import io.ilink.sdk.IlinkModels.SendResult;
import io.ilink.sdk.IlinkModels.SessionStatus;
import io.ilink.sdk.IlinkModels.Updates;
import io.ilink.sdk.IlinkModels.UploadedFile;

/**
 * An ergonomic client that merges default routing parameters into every call
 * and returns typed models from {@link IlinkModels}.
 *
 * <pre>{@code
 * IlinkClient client = new IlinkClient("cfe9a2017e77-im-bot");
 * List<Account> accounts = client.accounts();
 * SendResult sent = client.sendText("someone@im.wechat", "hello from Java");
 * System.out.println(sent.messageId);
 * }</pre>
 *
 * {@link #call(String, String)} and {@link #callValue(String, JsonValue)} remain
 * available for operations without a dedicated helper. See
 * {@code docs/reference/ffi.md} for every operation.
 */
public class IlinkClient {

    private final String accountId;
    private final String baseUrl;
    private final String cdnBaseUrl;
    private final String routeTag;
    private final String botAgent;
    private final int timeoutMs;

    public IlinkClient() {
        this(null, null, null, null, null, 0);
    }

    public IlinkClient(String accountId) {
        this(accountId, null, null, null, null, 0);
    }

    public IlinkClient(String accountId, String baseUrl, String cdnBaseUrl,
                       String routeTag, String botAgent, int timeoutMs) {
        this.accountId = accountId;
        this.baseUrl = baseUrl;
        this.cdnBaseUrl = cdnBaseUrl;
        this.routeTag = routeTag;
        this.botAgent = botAgent;
        this.timeoutMs = timeoutMs;
    }

    // -- plumbing -----------------------------------------------------------

    /** Run any operation with the client's routing parameters merged in. */
    public String call(String op, String paramsJson) {
        return IlinkSdk.call(op, mergeRouting(paramsJson));
    }

    /** Run any operation and return the parsed result. */
    public JsonValue callValue(String op, JsonValue params) {
        return JsonValue.parse(call(op, params == null ? "{}" : params.dump()));
    }

    // -- meta ---------------------------------------------------------------

    public String version() {
        return IlinkSdk.version();
    }

    public Paths paths() {
        return Paths.fromJson(callValue("paths", JsonValue.object()));
    }

    // -- accounts -----------------------------------------------------------

    public List<Account> accounts() {
        List<Account> out = new ArrayList<>();
        for (JsonValue item : callValue("accounts.list", JsonValue.object()).items()) {
            out.add(Account.fromJson(item));
        }
        return out;
    }

    public Account resolveAccount(String accountId) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        return Account.fromJson(callValue("account.resolve", params));
    }

    public Account saveAccount(String accountId, String token, String baseUrl, String userId) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        if (token != null) params.put("token", JsonValue.of(token));
        if (baseUrl != null) params.put("base_url", JsonValue.of(baseUrl));
        if (userId != null) params.put("user_id", JsonValue.of(userId));
        return Account.fromJson(callValue("account.save", params));
    }

    /** Forget an account and all of its local state. */
    public void clearAccount(String accountId) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        callValue("account.clear", params);
    }

    // -- QR login -----------------------------------------------------------

    public LoginStart loginStart() {
        return loginStart(null, null);
    }

    public LoginStart loginStart(String baseUrl, String botType) {
        JsonValue params = JsonValue.object();
        if (baseUrl != null) params.put("base_url", JsonValue.of(baseUrl));
        if (botType != null) params.put("bot_type", JsonValue.of(botType));
        return LoginStart.fromJson(callValue("login.start", params));
    }

    public LoginPoll loginPoll(String sessionKey) {
        JsonValue params = JsonValue.object();
        params.put("session_key", JsonValue.of(sessionKey));
        return LoginPoll.fromJson(callValue("login.poll", params));
    }

    public void loginSubmitVerifyCode(String sessionKey, String code) {
        JsonValue params = JsonValue.object();
        params.put("session_key", JsonValue.of(sessionKey));
        params.put("code", JsonValue.of(code));
        callValue("login.submit_verify_code", params);
    }

    public LoginStart loginRefresh(String sessionKey) {
        JsonValue params = JsonValue.object();
        params.put("session_key", JsonValue.of(sessionKey));
        return LoginStart.fromJson(callValue("login.refresh", params));
    }

    /** Wait for confirmation without persisting the credentials. */
    public LoginResult loginWait(String sessionKey) {
        return loginWait(sessionKey, 480, null);
    }

    public LoginResult loginWait(String sessionKey, int timeoutSecs, String verifyCode) {
        return LoginResult.fromJson(callValue("login.wait", loginParams(sessionKey, timeoutSecs, verifyCode)));
    }

    public LoginResult loginComplete(String sessionKey) {
        return loginComplete(sessionKey, 480, null);
    }

    /** Wait for confirmation and persist the credentials. */
    public LoginResult loginComplete(String sessionKey, int timeoutSecs, String verifyCode) {
        return LoginResult.fromJson(
                callValue("login.complete", loginParams(sessionKey, timeoutSecs, verifyCode)));
    }

    // -- outbound -----------------------------------------------------------

    public SendResult sendText(String to, String text) {
        return sendText(to, text, null);
    }

    public SendResult sendText(String to, String text, String contextToken) {
        JsonValue params = JsonValue.object();
        params.put("to", JsonValue.of(to));
        params.put("text", JsonValue.of(text));
        if (contextToken != null) params.put("context_token", JsonValue.of(contextToken));
        return SendResult.fromJson(callValue("send.text", params));
    }

    public SendResult sendFile(String to, String path, String caption) {
        JsonValue params = JsonValue.object();
        params.put("to", JsonValue.of(to));
        params.put("path", JsonValue.of(path));
        if (caption != null) params.put("caption", JsonValue.of(caption));
        return SendResult.fromJson(callValue("send.file", params));
    }

    /** Send in-memory bytes; they are base64 encoded for the wire. */
    public SendResult sendBytes(String to, String fileName, byte[] data, String caption) {
        JsonValue params = JsonValue.object();
        params.put("to", JsonValue.of(to));
        params.put("file_name", JsonValue.of(fileName));
        params.put("data_base64", JsonValue.of(Base64.getEncoder().encodeToString(data)));
        if (caption != null) params.put("caption", JsonValue.of(caption));
        return SendResult.fromJson(callValue("send.bytes", params));
    }

    /** Send a raw protocol item (advanced). */
    public SendResult sendItem(String to, JsonValue item, String contextToken) {
        JsonValue params = JsonValue.object();
        params.put("to", JsonValue.of(to));
        params.put("item", item);
        if (contextToken != null) params.put("context_token", JsonValue.of(contextToken));
        return SendResult.fromJson(callValue("send.item", params));
    }

    // -- inbound / API ------------------------------------------------------

    public Updates getUpdates() {
        return getUpdates(null, 0);
    }

    /** Long-poll for inbound messages; pass the previous cursor back in. */
    public Updates getUpdates(String getUpdatesBuf, int timeoutMs) {
        JsonValue params = JsonValue.object();
        if (getUpdatesBuf != null && !getUpdatesBuf.isEmpty()) {
            params.put("get_updates_buf", JsonValue.of(getUpdatesBuf));
        }
        if (timeoutMs > 0) params.put("timeout_ms", JsonValue.of(timeoutMs));
        return Updates.fromJson(callValue("get_updates", params));
    }

    public GetConfig getConfig(String userId, String contextToken) {
        JsonValue params = JsonValue.object();
        params.put("user_id", JsonValue.of(userId));
        if (contextToken != null) params.put("context_token", JsonValue.of(contextToken));
        return GetConfig.fromJson(callValue("get_config", params));
    }

    public void sendTyping(String userId, String typingTicket, String status) {
        JsonValue params = JsonValue.object();
        params.put("user_id", JsonValue.of(userId));
        params.put("typing_ticket", JsonValue.of(typingTicket));
        params.put("status", JsonValue.of(status == null ? "typing" : status));
        callValue("send_typing", params);
    }

    /** Lifecycle ping ({@code start} or {@code stop}). */
    public void notify(String action) {
        JsonValue params = JsonValue.object();
        params.put("action", JsonValue.of(action == null ? "start" : action));
        callValue("notify", params);
    }

    /** Parse a raw inbound message into a {@link MsgContext}. */
    public MsgContext contextToMessage(JsonValue message) {
        return contextToMessage(message, null);
    }

    public MsgContext contextToMessage(JsonValue message, String accountId) {
        JsonValue params = JsonValue.object();
        params.put("message", message);
        if (accountId != null) params.put("account_id", JsonValue.of(accountId));
        return MsgContext.fromJson(callValue("context.to_message", params));
    }

    /** Extract the display body from a list of message items. */
    public String messageBody(JsonValue items) {
        JsonValue params = JsonValue.object();
        params.put("items", items);
        return callValue("message.body", params).get("body").asString("");
    }

    // -- media --------------------------------------------------------------

    public UploadedFile upload(String to, String path, String mediaType) {
        JsonValue params = JsonValue.object();
        params.put("to", JsonValue.of(to));
        params.put("path", JsonValue.of(path));
        params.put("media_type", JsonValue.of(mediaType == null ? "file" : mediaType));
        return UploadedFile.fromJson(callValue("media.upload", params));
    }

    /** Download a CDN reference, decrypting it when {@code aesKey} is given. */
    public MediaFetch download(JsonValue media, String aesKey, String destPath) {
        JsonValue params = JsonValue.object();
        params.put("media", media);
        if (aesKey != null) params.put("aes_key", JsonValue.of(aesKey));
        if (destPath != null) params.put("dest_path", JsonValue.of(destPath));
        return MediaFetch.fromJson(callValue("media.download", params));
    }

    public MediaFetch fetch(String url, String destPath) {
        JsonValue params = JsonValue.object();
        params.put("url", JsonValue.of(url));
        if (destPath != null) params.put("dest_path", JsonValue.of(destPath));
        return MediaFetch.fromJson(callValue("media.fetch", params));
    }

    /** Download every media referenced by an inbound message item. */
    public InboundMedia downloadItem(JsonValue item, String destDir) {
        JsonValue params = JsonValue.object();
        params.put("item", item);
        if (destDir != null) params.put("dest_dir", JsonValue.of(destDir));
        return InboundMedia.fromJson(callValue("media.download_item", params));
    }

    // -- quote cache --------------------------------------------------------

    public QuotePolicy quotePolicy() {
        return QuotePolicy.fromJson(callValue("quote.policy", JsonValue.object()));
    }

    /** Return a cached quoted message, or {@code null} when absent. */
    public QuoteRecord quoteFind(String accountId, String conversationId, String messageId) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        params.put("conversation_id", JsonValue.of(conversationId));
        params.put("message_id", JsonValue.of(messageId));
        JsonValue result = callValue("quote.find", params);
        return result.isObject() ? QuoteRecord.fromJson(result) : null;
    }

    public void quotePut(QuoteRecord record) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(record.accountId));
        params.put("conversation_id", JsonValue.of(record.conversationId));
        params.put("message_id", JsonValue.of(record.messageId));
        params.put("body", JsonValue.of(record.body));
        if (record.direction != null && !record.direction.isEmpty()) {
            params.put("direction", JsonValue.of(record.direction));
        }
        if (record.mediaPath != null) params.put("source_media_path", JsonValue.of(record.mediaPath));
        if (record.mediaMime != null) params.put("media_mime", JsonValue.of(record.mediaMime));
        if (record.mediaName != null) params.put("media_name", JsonValue.of(record.mediaName));
        if (record.createdAt != 0) params.put("created_at", JsonValue.of(record.createdAt));
        callValue("quote.put", params);
    }

    public void quoteGc() {
        callValue("quote.gc", JsonValue.object());
    }

    public void quoteDeleteAccount(String accountId) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        callValue("quote.delete_account", params);
    }

    public String quoteMediaDir(String accountId) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        return callValue("quote.media_dir", params).get("path").asString("");
    }

    // -- context tokens -----------------------------------------------------

    /** Return the stored context token, or {@code null} when absent. */
    public String contextGet(String accountId, String userId) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        params.put("user_id", JsonValue.of(userId));
        JsonValue result = callValue("context.get", params);
        return result.isObject() ? result.get("token").asString("") : null;
    }

    public void contextSet(String accountId, String userId, String token) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        params.put("user_id", JsonValue.of(userId));
        params.put("token", JsonValue.of(token));
        callValue("context.set", params);
    }

    public void contextClear(String accountId) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        callValue("context.clear", params);
    }

    public long contextRestore(String accountId) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        return callValue("context.restore", params).get("restored").asLong(0);
    }

    // -- sync cursor --------------------------------------------------------

    public String syncLoad(String accountId) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        return callValue("sync.load", params).get("get_updates_buf").asString("");
    }

    public void syncSave(String accountId, String buf) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        params.put("buf", JsonValue.of(buf));
        callValue("sync.save", params);
    }

    public void syncClear(String accountId) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        callValue("sync.clear", params);
    }

    // -- session guard ------------------------------------------------------

    public SessionStatus sessionStatus() {
        return sessionStatus(null);
    }

    public SessionStatus sessionStatus(String accountId) {
        JsonValue params = JsonValue.object();
        if (accountId != null) params.put("account_id", JsonValue.of(accountId));
        return SessionStatus.fromJson(callValue("session.status", params));
    }

    public void sessionPause(String accountId) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        callValue("session.pause", params);
    }

    public void sessionResume(String accountId) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        callValue("session.resume", params);
    }

    /** Throw {@link IlinkError} when the session is paused. */
    public void sessionAssert(String accountId) {
        JsonValue params = JsonValue.object();
        params.put("account_id", JsonValue.of(accountId));
        callValue("session.assert", params);
    }

    // -- helpers ------------------------------------------------------------

    public MimeInfo mimeInfo(String path) {
        JsonValue params = JsonValue.object();
        params.put("path", JsonValue.of(path));
        return MimeInfo.fromJson(callValue("mime.info", params));
    }

    public String mimeExtension(String mime) {
        JsonValue params = JsonValue.object();
        params.put("mime", JsonValue.of(mime));
        return callValue("mime.extension", params).get("extension").asString("");
    }

    public String generateId(String prefix) {
        JsonValue params = JsonValue.object();
        params.put("prefix", JsonValue.of(prefix == null ? "ilink-sdk" : prefix));
        return callValue("util.generate_id", params).get("id").asString("");
    }

    public String fileKey() {
        return callValue("util.file_key", JsonValue.object()).get("file_key").asString("");
    }

    public String tempName(String prefix, String ext) {
        JsonValue params = JsonValue.object();
        params.put("prefix", JsonValue.of(prefix == null ? "ilink-sdk" : prefix));
        params.put("ext", JsonValue.of(ext == null ? "" : ext));
        return callValue("util.temp_name", params).get("name").asString("");
    }

    public ConfigLoad configLoad() {
        return ConfigLoad.fromJson(callValue("config.load", JsonValue.object()));
    }

    // -- crypto -------------------------------------------------------------

    public CryptoResult cryptoEncrypt(String keyHex, byte[] data) {
        JsonValue params = JsonValue.object();
        params.put("key_hex", JsonValue.of(keyHex));
        params.put("data_base64", JsonValue.of(Base64.getEncoder().encodeToString(data)));
        return CryptoResult.fromJson(callValue("crypto.encrypt", params));
    }

    public byte[] cryptoDecrypt(String keyHex, String dataBase64) {
        JsonValue params = JsonValue.object();
        params.put("key_hex", JsonValue.of(keyHex));
        params.put("data_base64", JsonValue.of(dataBase64));
        String out = callValue("crypto.decrypt", params).get("data_base64").asString("");
        return Base64.getDecoder().decode(out);
    }

    // -- internals ----------------------------------------------------------

    private JsonValue loginParams(String sessionKey, int timeoutSecs, String verifyCode) {
        JsonValue params = JsonValue.object();
        params.put("session_key", JsonValue.of(sessionKey));
        params.put("timeout_secs", JsonValue.of(timeoutSecs));
        if (verifyCode != null) params.put("verify_code", JsonValue.of(verifyCode));
        return params;
    }

    private String mergeRouting(String paramsJson) {
        StringBuilder extra = new StringBuilder();
        appendRouting(extra, "account_id", accountId);
        appendRouting(extra, "base_url", baseUrl);
        appendRouting(extra, "cdn_base_url", cdnBaseUrl);
        appendRouting(extra, "route_tag", routeTag);
        appendRouting(extra, "bot_agent", botAgent);
        if (timeoutMs > 0) {
            if (extra.length() > 0) {
                extra.append(',');
            }
            extra.append("\"timeout_ms\":").append(timeoutMs);
        }
        if (extra.length() == 0) {
            return paramsJson == null || paramsJson.isEmpty() ? "{}" : paramsJson;
        }
        if (paramsJson == null || paramsJson.isEmpty() || paramsJson.equals("{}")) {
            return "{" + extra + "}";
        }
        int brace = paramsJson.indexOf('{');
        if (brace < 0) {
            return paramsJson;
        }
        return paramsJson.substring(0, brace + 1) + extra + "," + paramsJson.substring(brace + 1);
    }

    private static void appendRouting(StringBuilder out, String key, String value) {
        if (value == null || value.isEmpty()) {
            return;
        }
        if (out.length() > 0) {
            out.append(',');
        }
        out.append('"').append(key).append("\":").append(Json.quote(value));
    }
}
