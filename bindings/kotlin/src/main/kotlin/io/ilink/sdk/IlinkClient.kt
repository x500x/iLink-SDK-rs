package io.ilink.sdk

import io.ilink.sdk.IlinkModels.Account
import io.ilink.sdk.IlinkModels.ConfigLoad
import io.ilink.sdk.IlinkModels.CryptoResult
import io.ilink.sdk.IlinkModels.GetConfig
import io.ilink.sdk.IlinkModels.InboundMedia
import io.ilink.sdk.IlinkModels.LoginPoll
import io.ilink.sdk.IlinkModels.LoginResult
import io.ilink.sdk.IlinkModels.LoginStart
import io.ilink.sdk.IlinkModels.MediaFetch
import io.ilink.sdk.IlinkModels.MimeInfo
import io.ilink.sdk.IlinkModels.MsgContext
import io.ilink.sdk.IlinkModels.Paths
import io.ilink.sdk.IlinkModels.QuotePolicy
import io.ilink.sdk.IlinkModels.QuoteRecord
import io.ilink.sdk.IlinkModels.SendResult
import io.ilink.sdk.IlinkModels.SessionStatus
import io.ilink.sdk.IlinkModels.Updates
import io.ilink.sdk.IlinkModels.UploadedFile
import java.util.Base64

/**
 * An ergonomic client that merges default routing parameters into every call
 * and returns typed models from [IlinkModels].
 *
 * ```kotlin
 * val client = IlinkClient(accountId = "cfe9a2017e77-im-bot")
 * val accounts = client.accounts()
 * val sent = client.sendText("someone@im.wechat", "hello from Kotlin")
 * println(sent.messageId)
 * ```
 *
 * [call] and [callValue] remain available for operations without a dedicated
 * helper. See `docs/reference/ffi.md` for every operation.
 */
class IlinkClient @JvmOverloads constructor(
    private val accountId: String? = null,
    private val baseUrl: String? = null,
    private val cdnBaseUrl: String? = null,
    private val routeTag: String? = null,
    private val botAgent: String? = null,
    private val timeoutMs: Int = 0,
) {

    // -- plumbing -----------------------------------------------------------

    /** Run any operation with the client's routing parameters merged in. */
    @JvmOverloads
    fun call(op: String, paramsJson: String? = "{}"): String =
        IlinkSdk.call(op, mergeRouting(paramsJson))

    /** Run any operation and return the parsed result. */
    fun callValue(op: String, params: JsonValue = JsonValue.objectValue()): JsonValue =
        JsonValue.parse(call(op, params.dump()))

    // -- meta ---------------------------------------------------------------

    fun version(): String = IlinkSdk.version()

    fun paths(): Paths = Paths.fromJson(callValue("paths"))

    // -- accounts -----------------------------------------------------------

    fun accounts(): List<Account> = callValue("accounts.list").items.map { Account.fromJson(it) }

    fun resolveAccount(accountId: String): Account {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        return Account.fromJson(callValue("account.resolve", params))
    }

    @JvmOverloads
    fun saveAccount(
        accountId: String,
        token: String? = null,
        baseUrl: String? = null,
        userId: String? = null,
    ): Account {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        token?.let { params.put("token", JsonValue.of(it)) }
        baseUrl?.let { params.put("base_url", JsonValue.of(it)) }
        userId?.let { params.put("user_id", JsonValue.of(it)) }
        return Account.fromJson(callValue("account.save", params))
    }

    /** Forget an account and all of its local state. */
    fun clearAccount(accountId: String) {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        callValue("account.clear", params)
    }

    // -- QR login -----------------------------------------------------------

    @JvmOverloads
    fun loginStart(baseUrl: String? = null, botType: String? = null): LoginStart {
        val params = JsonValue.objectValue()
        baseUrl?.let { params.put("base_url", JsonValue.of(it)) }
        botType?.let { params.put("bot_type", JsonValue.of(it)) }
        return LoginStart.fromJson(callValue("login.start", params))
    }

    fun loginPoll(sessionKey: String): LoginPoll {
        val params = JsonValue.objectValue()
        params.put("session_key", JsonValue.of(sessionKey))
        return LoginPoll.fromJson(callValue("login.poll", params))
    }

    fun loginSubmitVerifyCode(sessionKey: String, code: String) {
        val params = JsonValue.objectValue()
        params.put("session_key", JsonValue.of(sessionKey))
        params.put("code", JsonValue.of(code))
        callValue("login.submit_verify_code", params)
    }

    fun loginRefresh(sessionKey: String): LoginStart {
        val params = JsonValue.objectValue()
        params.put("session_key", JsonValue.of(sessionKey))
        return LoginStart.fromJson(callValue("login.refresh", params))
    }

    /** Wait for confirmation without persisting the credentials. */
    @JvmOverloads
    fun loginWait(sessionKey: String, timeoutSecs: Int = 480, verifyCode: String? = null): LoginResult =
        LoginResult.fromJson(callValue("login.wait", loginParams(sessionKey, timeoutSecs, verifyCode)))

    /** Wait for confirmation and persist the credentials. */
    @JvmOverloads
    fun loginComplete(sessionKey: String, timeoutSecs: Int = 480, verifyCode: String? = null): LoginResult =
        LoginResult.fromJson(callValue("login.complete", loginParams(sessionKey, timeoutSecs, verifyCode)))

    // -- outbound -----------------------------------------------------------

    @JvmOverloads
    fun sendText(to: String, text: String, contextToken: String? = null): SendResult {
        val params = JsonValue.objectValue()
        params.put("to", JsonValue.of(to))
        params.put("text", JsonValue.of(text))
        contextToken?.let { params.put("context_token", JsonValue.of(it)) }
        return SendResult.fromJson(callValue("send.text", params))
    }

    @JvmOverloads
    fun sendFile(to: String, path: String, caption: String? = null): SendResult {
        val params = JsonValue.objectValue()
        params.put("to", JsonValue.of(to))
        params.put("path", JsonValue.of(path))
        caption?.let { params.put("caption", JsonValue.of(it)) }
        return SendResult.fromJson(callValue("send.file", params))
    }

    /** Send in-memory bytes; they are base64 encoded for the wire. */
    @JvmOverloads
    fun sendBytes(to: String, fileName: String, data: ByteArray, caption: String? = null): SendResult {
        val params = JsonValue.objectValue()
        params.put("to", JsonValue.of(to))
        params.put("file_name", JsonValue.of(fileName))
        params.put("data_base64", JsonValue.of(Base64.getEncoder().encodeToString(data)))
        caption?.let { params.put("caption", JsonValue.of(it)) }
        return SendResult.fromJson(callValue("send.bytes", params))
    }

    /** Send a raw protocol item (advanced). */
    @JvmOverloads
    fun sendItem(to: String, item: JsonValue, contextToken: String? = null): SendResult {
        val params = JsonValue.objectValue()
        params.put("to", JsonValue.of(to))
        params.put("item", item)
        contextToken?.let { params.put("context_token", JsonValue.of(it)) }
        return SendResult.fromJson(callValue("send.item", params))
    }

    // -- inbound / API ------------------------------------------------------

    @JvmOverloads
    fun getUpdates(getUpdatesBuf: String? = null, timeoutMs: Int = 0): Updates {
        val params = JsonValue.objectValue()
        if (!getUpdatesBuf.isNullOrEmpty()) params.put("get_updates_buf", JsonValue.of(getUpdatesBuf))
        if (timeoutMs > 0) params.put("timeout_ms", JsonValue.of(timeoutMs))
        return Updates.fromJson(callValue("get_updates", params))
    }

    @JvmOverloads
    fun getConfig(userId: String, contextToken: String? = null): GetConfig {
        val params = JsonValue.objectValue()
        params.put("user_id", JsonValue.of(userId))
        contextToken?.let { params.put("context_token", JsonValue.of(it)) }
        return GetConfig.fromJson(callValue("get_config", params))
    }

    @JvmOverloads
    fun sendTyping(userId: String, typingTicket: String, status: String = "typing") {
        val params = JsonValue.objectValue()
        params.put("user_id", JsonValue.of(userId))
        params.put("typing_ticket", JsonValue.of(typingTicket))
        params.put("status", JsonValue.of(status))
        callValue("send_typing", params)
    }

    /**
     * Lifecycle ping (`start` or `stop`).
     *
     * No default argument: `@JvmOverloads` would generate a `notify()` overload
     * clashing with [Object.notify].
     */
    fun notify(action: String) {
        val params = JsonValue.objectValue()
        params.put("action", JsonValue.of(action))
        callValue("notify", params)
    }

    /** Parse a raw inbound message into a [MsgContext]. */
    @JvmOverloads
    fun contextToMessage(message: JsonValue, accountId: String? = null): MsgContext {
        val params = JsonValue.objectValue()
        params.put("message", message)
        accountId?.let { params.put("account_id", JsonValue.of(it)) }
        return MsgContext.fromJson(callValue("context.to_message", params))
    }

    /** Extract the display body from a list of message items. */
    fun messageBody(items: JsonValue): String {
        val params = JsonValue.objectValue()
        params.put("items", items)
        return callValue("message.body", params)["body"].asString()
    }

    // -- media --------------------------------------------------------------

    @JvmOverloads
    fun upload(to: String, path: String, mediaType: String = "file"): UploadedFile {
        val params = JsonValue.objectValue()
        params.put("to", JsonValue.of(to))
        params.put("path", JsonValue.of(path))
        params.put("media_type", JsonValue.of(mediaType))
        return UploadedFile.fromJson(callValue("media.upload", params))
    }

    /** Download a CDN reference, decrypting it when [aesKey] is given. */
    @JvmOverloads
    fun download(media: JsonValue, aesKey: String? = null, destPath: String? = null): MediaFetch {
        val params = JsonValue.objectValue()
        params.put("media", media)
        aesKey?.let { params.put("aes_key", JsonValue.of(it)) }
        destPath?.let { params.put("dest_path", JsonValue.of(it)) }
        return MediaFetch.fromJson(callValue("media.download", params))
    }

    @JvmOverloads
    fun fetch(url: String, destPath: String? = null): MediaFetch {
        val params = JsonValue.objectValue()
        params.put("url", JsonValue.of(url))
        destPath?.let { params.put("dest_path", JsonValue.of(it)) }
        return MediaFetch.fromJson(callValue("media.fetch", params))
    }

    /** Download every media referenced by an inbound message item. */
    @JvmOverloads
    fun downloadItem(item: JsonValue, destDir: String? = null): InboundMedia {
        val params = JsonValue.objectValue()
        params.put("item", item)
        destDir?.let { params.put("dest_dir", JsonValue.of(it)) }
        return InboundMedia.fromJson(callValue("media.download_item", params))
    }

    // -- quote cache --------------------------------------------------------

    fun quotePolicy(): QuotePolicy = QuotePolicy.fromJson(callValue("quote.policy"))

    /** Return a cached quoted message, or `null` when absent. */
    fun quoteFind(accountId: String, conversationId: String, messageId: String): QuoteRecord? {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        params.put("conversation_id", JsonValue.of(conversationId))
        params.put("message_id", JsonValue.of(messageId))
        val result = callValue("quote.find", params)
        return if (result.isObject) QuoteRecord.fromJson(result) else null
    }

    fun quotePut(record: QuoteRecord) {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(record.accountId))
        params.put("conversation_id", JsonValue.of(record.conversationId))
        params.put("message_id", JsonValue.of(record.messageId))
        params.put("body", JsonValue.of(record.body))
        if (record.direction.isNotEmpty()) params.put("direction", JsonValue.of(record.direction))
        record.mediaPath?.let { params.put("source_media_path", JsonValue.of(it)) }
        record.mediaMime?.let { params.put("media_mime", JsonValue.of(it)) }
        record.mediaName?.let { params.put("media_name", JsonValue.of(it)) }
        if (record.createdAt != 0L) params.put("created_at", JsonValue.of(record.createdAt))
        callValue("quote.put", params)
    }

    fun quoteGc() {
        callValue("quote.gc")
    }

    fun quoteDeleteAccount(accountId: String) {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        callValue("quote.delete_account", params)
    }

    fun quoteMediaDir(accountId: String): String {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        return callValue("quote.media_dir", params)["path"].asString()
    }

    // -- context tokens -----------------------------------------------------

    /** Return the stored context token, or `null` when absent. */
    fun contextGet(accountId: String, userId: String): String? {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        params.put("user_id", JsonValue.of(userId))
        val result = callValue("context.get", params)
        return if (result.isObject) result["token"].asString() else null
    }

    fun contextSet(accountId: String, userId: String, token: String) {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        params.put("user_id", JsonValue.of(userId))
        params.put("token", JsonValue.of(token))
        callValue("context.set", params)
    }

    fun contextClear(accountId: String) {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        callValue("context.clear", params)
    }

    fun contextRestore(accountId: String): Long {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        return callValue("context.restore", params)["restored"].asLong()
    }

    // -- sync cursor --------------------------------------------------------

    fun syncLoad(accountId: String): String {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        return callValue("sync.load", params)["get_updates_buf"].asString()
    }

    fun syncSave(accountId: String, buf: String) {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        params.put("buf", JsonValue.of(buf))
        callValue("sync.save", params)
    }

    fun syncClear(accountId: String) {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        callValue("sync.clear", params)
    }

    // -- session guard ------------------------------------------------------

    @JvmOverloads
    fun sessionStatus(accountId: String? = null): SessionStatus {
        val params = JsonValue.objectValue()
        accountId?.let { params.put("account_id", JsonValue.of(it)) }
        return SessionStatus.fromJson(callValue("session.status", params))
    }

    fun sessionPause(accountId: String) {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        callValue("session.pause", params)
    }

    fun sessionResume(accountId: String) {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        callValue("session.resume", params)
    }

    /** Throw [IlinkError] when the session is paused. */
    fun sessionAssert(accountId: String) {
        val params = JsonValue.objectValue()
        params.put("account_id", JsonValue.of(accountId))
        callValue("session.assert", params)
    }

    // -- helpers ------------------------------------------------------------

    fun mimeInfo(path: String): MimeInfo {
        val params = JsonValue.objectValue()
        params.put("path", JsonValue.of(path))
        return MimeInfo.fromJson(callValue("mime.info", params))
    }

    fun mimeExtension(mime: String): String {
        val params = JsonValue.objectValue()
        params.put("mime", JsonValue.of(mime))
        return callValue("mime.extension", params)["extension"].asString()
    }

    @JvmOverloads
    fun generateId(prefix: String = "ilink-sdk"): String {
        val params = JsonValue.objectValue()
        params.put("prefix", JsonValue.of(prefix))
        return callValue("util.generate_id", params)["id"].asString()
    }

    fun fileKey(): String = callValue("util.file_key")["file_key"].asString()

    @JvmOverloads
    fun tempName(prefix: String = "ilink-sdk", ext: String = ""): String {
        val params = JsonValue.objectValue()
        params.put("prefix", JsonValue.of(prefix))
        params.put("ext", JsonValue.of(ext))
        return callValue("util.temp_name", params)["name"].asString()
    }

    fun configLoad(): ConfigLoad = ConfigLoad.fromJson(callValue("config.load"))

    // -- crypto -------------------------------------------------------------

    fun cryptoEncrypt(keyHex: String, data: ByteArray): CryptoResult {
        val params = JsonValue.objectValue()
        params.put("key_hex", JsonValue.of(keyHex))
        params.put("data_base64", JsonValue.of(Base64.getEncoder().encodeToString(data)))
        return CryptoResult.fromJson(callValue("crypto.encrypt", params))
    }

    fun cryptoDecrypt(keyHex: String, dataBase64: String): ByteArray {
        val params = JsonValue.objectValue()
        params.put("key_hex", JsonValue.of(keyHex))
        params.put("data_base64", JsonValue.of(dataBase64))
        val out = callValue("crypto.decrypt", params)["data_base64"].asString()
        return Base64.getDecoder().decode(out)
    }

    // -- internals ----------------------------------------------------------

    private fun loginParams(sessionKey: String, timeoutSecs: Int, verifyCode: String?): JsonValue {
        val params = JsonValue.objectValue()
        params.put("session_key", JsonValue.of(sessionKey))
        params.put("timeout_secs", JsonValue.of(timeoutSecs))
        verifyCode?.let { params.put("verify_code", JsonValue.of(it)) }
        return params
    }

    private fun mergeRouting(paramsJson: String?): String {
        val extra = StringBuilder()
        appendRouting(extra, "account_id", accountId)
        appendRouting(extra, "base_url", baseUrl)
        appendRouting(extra, "cdn_base_url", cdnBaseUrl)
        appendRouting(extra, "route_tag", routeTag)
        appendRouting(extra, "bot_agent", botAgent)
        if (timeoutMs > 0) {
            if (extra.isNotEmpty()) extra.append(',')
            extra.append("\"timeout_ms\":").append(timeoutMs)
        }
        if (extra.isEmpty()) return if (paramsJson.isNullOrEmpty()) "{}" else paramsJson
        if (paramsJson.isNullOrEmpty() || paramsJson == "{}") return "{$extra}"
        val brace = paramsJson.indexOf('{')
        if (brace < 0) return paramsJson
        return paramsJson.substring(0, brace + 1) + extra + "," + paramsJson.substring(brace + 1)
    }

    private fun appendRouting(out: StringBuilder, key: String, value: String?) {
        if (value.isNullOrEmpty()) return
        if (out.isNotEmpty()) out.append(',')
        out.append('"').append(key).append("\":").append(Json.quote(value))
    }
}
