package io.ilink.sdk;

/**
 * An ergonomic client that merges default routing parameters into every call.
 *
 * <pre>{@code
 * IlinkClient client = new IlinkClient("cfe9a2017e77-im-bot");
 * String accounts = client.accounts();
 * String sent = client.sendText("someone@im.wechat", "hello from Java");
 * }</pre>
 *
 * Results are raw JSON strings so callers can use their preferred JSON library
 * (Gson, Jackson, ...). See {@code docs/reference/ffi.md} for every operation.
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

    /** Run any operation with the client's routing parameters merged in. */
    public String call(String op, String paramsJson) {
        return IlinkSdk.call(op, mergeRouting(paramsJson));
    }

    public String version() {
        return IlinkSdk.version();
    }

    public String accounts() {
        return call("accounts.list", "{}");
    }

    public String loginStart() {
        return call("login.start", "{}");
    }

    public String loginComplete(String sessionKey) {
        return loginComplete(sessionKey, 480, null);
    }

    public String loginComplete(String sessionKey, int timeoutSecs, String verifyCode) {
        StringBuilder params = new StringBuilder()
                .append("{\"session_key\":").append(Json.quote(sessionKey))
                .append(",\"timeout_secs\":").append(timeoutSecs);
        if (verifyCode != null && !verifyCode.isEmpty()) {
            params.append(",\"verify_code\":").append(Json.quote(verifyCode));
        }
        return call("login.complete", params.append('}').toString());
    }

    public String sendText(String to, String text) {
        return sendText(to, text, null);
    }

    public String sendText(String to, String text, String contextToken) {
        StringBuilder params = new StringBuilder()
                .append("{\"to\":").append(Json.quote(to))
                .append(",\"text\":").append(Json.quote(text));
        if (contextToken != null && !contextToken.isEmpty()) {
            params.append(",\"context_token\":").append(Json.quote(contextToken));
        }
        return call("send.text", params.append('}').toString());
    }

    public String sendFile(String to, String path, String caption) {
        StringBuilder params = new StringBuilder()
                .append("{\"to\":").append(Json.quote(to))
                .append(",\"path\":").append(Json.quote(path));
        if (caption != null && !caption.isEmpty()) {
            params.append(",\"caption\":").append(Json.quote(caption));
        }
        return call("send.file", params.append('}').toString());
    }

    public String getUpdates(String getUpdatesBuf, int timeoutMs) {
        StringBuilder params = new StringBuilder("{");
        if (getUpdatesBuf != null && !getUpdatesBuf.isEmpty()) {
            params.append("\"get_updates_buf\":").append(Json.quote(getUpdatesBuf));
        }
        if (timeoutMs > 0) {
            if (params.length() > 1) {
                params.append(',');
            }
            params.append("\"timeout_ms\":").append(timeoutMs);
        }
        return call("get_updates", params.append('}').toString());
    }

    public String sendTyping(String userId, String typingTicket, String status) {
        return call("send_typing", "{\"user_id\":" + Json.quote(userId)
                + ",\"typing_ticket\":" + Json.quote(typingTicket)
                + ",\"status\":" + Json.quote(status) + "}");
    }

    public String upload(String to, String path, String mediaType) {
        return call("media.upload", "{\"to\":" + Json.quote(to)
                + ",\"path\":" + Json.quote(path)
                + ",\"media_type\":" + Json.quote(mediaType) + "}");
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
