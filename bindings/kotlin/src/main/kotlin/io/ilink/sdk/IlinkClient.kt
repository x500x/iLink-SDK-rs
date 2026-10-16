package io.ilink.sdk

/**
 * An ergonomic client that merges default routing parameters into every call.
 *
 * ```kotlin
 * val client = IlinkClient(accountId = "cfe9a2017e77-im-bot")
 * val accounts = client.accounts()
 * val sent = client.sendText("someone@im.wechat", "hello from Kotlin")
 * ```
 *
 * Results are raw JSON strings so callers can use their preferred JSON library
 * (kotlinx.serialization, Gson, Jackson, ...). See `docs/reference/ffi.md` for
 * every operation.
 */
class IlinkClient @JvmOverloads constructor(
    private val accountId: String? = null,
    private val baseUrl: String? = null,
    private val cdnBaseUrl: String? = null,
    private val routeTag: String? = null,
    private val botAgent: String? = null,
    private val timeoutMs: Int = 0,
) {

    /** Run any operation with the client's routing parameters merged in. */
    @JvmOverloads
    fun call(op: String, paramsJson: String? = "{}"): String =
        IlinkSdk.call(op, mergeRouting(paramsJson))

    fun version(): String = IlinkSdk.version()

    fun accounts(): String = call("accounts.list", "{}")

    fun loginStart(): String = call("login.start", "{}")

    @JvmOverloads
    fun loginComplete(sessionKey: String, timeoutSecs: Int = 480, verifyCode: String? = null): String {
        val params = StringBuilder()
            .append("{\"session_key\":").append(Json.quote(sessionKey))
            .append(",\"timeout_secs\":").append(timeoutSecs)
        if (!verifyCode.isNullOrEmpty()) {
            params.append(",\"verify_code\":").append(Json.quote(verifyCode))
        }
        return call("login.complete", params.append('}').toString())
    }

    @JvmOverloads
    fun sendText(to: String, text: String, contextToken: String? = null): String {
        val params = StringBuilder()
            .append("{\"to\":").append(Json.quote(to))
            .append(",\"text\":").append(Json.quote(text))
        if (!contextToken.isNullOrEmpty()) {
            params.append(",\"context_token\":").append(Json.quote(contextToken))
        }
        return call("send.text", params.append('}').toString())
    }

    @JvmOverloads
    fun sendFile(to: String, path: String, caption: String? = null): String {
        val params = StringBuilder()
            .append("{\"to\":").append(Json.quote(to))
            .append(",\"path\":").append(Json.quote(path))
        if (!caption.isNullOrEmpty()) {
            params.append(",\"caption\":").append(Json.quote(caption))
        }
        return call("send.file", params.append('}').toString())
    }

    @JvmOverloads
    fun getUpdates(getUpdatesBuf: String? = null, timeoutMs: Int = 0): String {
        val params = StringBuilder("{")
        if (!getUpdatesBuf.isNullOrEmpty()) {
            params.append("\"get_updates_buf\":").append(Json.quote(getUpdatesBuf))
        }
        if (timeoutMs > 0) {
            if (params.length > 1) params.append(',')
            params.append("\"timeout_ms\":").append(timeoutMs)
        }
        return call("get_updates", params.append('}').toString())
    }

    @JvmOverloads
    fun sendTyping(userId: String, typingTicket: String, status: String = "typing"): String =
        call(
            "send_typing",
            "{\"user_id\":" + Json.quote(userId) +
                ",\"typing_ticket\":" + Json.quote(typingTicket) +
                ",\"status\":" + Json.quote(status) + "}",
        )

    @JvmOverloads
    fun upload(to: String, path: String, mediaType: String = "file"): String =
        call(
            "media.upload",
            "{\"to\":" + Json.quote(to) +
                ",\"path\":" + Json.quote(path) +
                ",\"media_type\":" + Json.quote(mediaType) + "}",
        )

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
