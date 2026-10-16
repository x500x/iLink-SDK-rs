// ilink_sdk.hpp — header-only C++ wrapper for the iLink (Weixin) SDK.
//
// Wraps the shared library's JSON C ABI in RAII + exceptions. The low-level
// `ilink::call` returns the raw `result` JSON string; `ilink::call_json`
// returns it parsed into an `ilink::json::Value`. `ilink::Client` adds typed
// helpers that decode results into the models from `ilink_types.hpp`:
//
//     #include "ilink_sdk.hpp"
//     ilink::Client client("cfe9a2017e77-im-bot");
//     ilink::SendResult sent = client.send_text("someone@im.wechat", "hi");
//     std::cout << sent.message_id << "\n";
//
// Failures throw `ilink::Error`, carrying the Rust `kind`.
//
// Compile with `-I include` and link the shared library, e.g.
//     g++ -std=c++17 -I include app.cpp target/release/ilink_sdk.dll -o app

#pragma once

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include "ilink_sdk.h"
#include "ilink_json.hpp"
#include "ilink_types.hpp"

namespace ilink {

/// Thrown when an operation fails. `kind()` mirrors `ilink_sdk::ErrorKind`.
class Error : public std::runtime_error {
public:
    Error(std::string kind, const std::string& message)
        : std::runtime_error(message), kind_(std::move(kind)) {}

    const std::string& kind() const noexcept { return kind_; }

private:
    std::string kind_;
};

namespace detail {

inline void skip_ws(const std::string& s, std::size_t& i) {
    while (i < s.size()) {
        const char c = s[i];
        if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
            ++i;
            continue;
        }
        break;
    }
}

inline std::size_t skip_string(const std::string& s, std::size_t i) {
    ++i; // opening quote
    while (i < s.size()) {
        const char c = s[i];
        if (c == '\\') {
            i += 2;
            continue;
        }
        if (c == '"') return i + 1;
        ++i;
    }
    throw Error("unknown", "malformed JSON string in SDK response");
}

inline std::size_t skip_value(const std::string& s, std::size_t i) {
    skip_ws(s, i);
    if (i >= s.size()) throw Error("unknown", "truncated JSON value");
    const char c = s[i];
    if (c == '"') return skip_string(s, i);
    if (c == '{' || c == '[') {
        int depth = 0;
        while (i < s.size()) {
            const char d = s[i];
            if (d == '"') {
                i = skip_string(s, i);
                continue;
            }
            if (d == '{' || d == '[') {
                ++depth;
                ++i;
                continue;
            }
            if (d == '}' || d == ']') {
                --depth;
                ++i;
                if (depth == 0) return i;
                continue;
            }
            ++i;
        }
        throw Error("unknown", "unbalanced JSON value in SDK response");
    }
    while (i < s.size() && s[i] != ',' && s[i] != '}' && s[i] != ']') ++i;
    while (i > 0 && (s[i - 1] == ' ' || s[i - 1] == '\t' || s[i - 1] == '\n' || s[i - 1] == '\r')) --i;
    return i;
}

struct Field {
    std::size_t begin = 0;
    std::size_t end = 0;
    bool found = false;
};

/// Locate a top-level key's value span in a flat JSON object.
inline Field top_level_field(const std::string& s, const std::string& key) {
    std::size_t i = 0;
    skip_ws(s, i);
    if (i >= s.size() || s[i] != '{') throw Error("unknown", "SDK response is not a JSON object");
    ++i;
    while (true) {
        skip_ws(s, i);
        if (i < s.size() && s[i] == '}') break;
        if (i >= s.size() || s[i] != '"') throw Error("unknown", "malformed JSON object");
        const std::size_t key_end = skip_string(s, i);
        const std::string found_key = s.substr(i + 1, key_end - i - 2);
        i = key_end;
        skip_ws(s, i);
        if (i >= s.size() || s[i] != ':') throw Error("unknown", "malformed JSON object");
        ++i;
        skip_ws(s, i);
        const std::size_t value_begin = i;
        const std::size_t value_end = skip_value(s, i);
        if (found_key == key) return Field{value_begin, value_end, true};
        i = value_end;
        skip_ws(s, i);
        if (i < s.size() && s[i] == ',') {
            ++i;
            continue;
        }
        break;
    }
    return Field{};
}

/// Decode a JSON string literal (including the surrounding quotes).
inline std::string decode_json_string(const std::string& literal) {
    std::string out;
    for (std::size_t i = 1; i + 1 < literal.size(); ++i) {
        const char c = literal[i];
        if (c != '\\') {
            out.push_back(c);
            continue;
        }
        if (++i + 1 > literal.size()) break;
        switch (literal[i]) {
            case '"': out.push_back('"'); break;
            case '\\': out.push_back('\\'); break;
            case '/': out.push_back('/'); break;
            case 'n': out.push_back('\n'); break;
            case 't': out.push_back('\t'); break;
            case 'r': out.push_back('\r'); break;
            case 'b': out.push_back('\b'); break;
            case 'f': out.push_back('\f'); break;
            case 'u': {
                if (i + 4 >= literal.size()) break;
                unsigned code = 0;
                for (int k = 1; k <= 4; ++k) {
                    const char h = literal[i + k];
                    code <<= 4;
                    if (h >= '0' && h <= '9') code |= unsigned(h - '0');
                    else if (h >= 'a' && h <= 'f') code |= unsigned(h - 'a' + 10);
                    else if (h >= 'A' && h <= 'F') code |= unsigned(h - 'A' + 10);
                }
                i += 4;
                if (code < 0x80) {
                    out.push_back(char(code));
                } else if (code < 0x800) {
                    out.push_back(char(0xC0 | (code >> 6)));
                    out.push_back(char(0x80 | (code & 0x3F)));
                } else {
                    out.push_back(char(0xE0 | (code >> 12)));
                    out.push_back(char(0x80 | ((code >> 6) & 0x3F)));
                    out.push_back(char(0x80 | (code & 0x3F)));
                }
                break;
            }
            default: out.push_back(literal[i]); break;
        }
    }
    return out;
}

/// JSON-escape a string value.
inline std::string quote(const std::string& s) {
    std::string out = "\"";
    for (char c : s) {
        switch (c) {
            case '"': out += "\\\""; break;
            case '\\': out += "\\\\"; break;
            case '\n': out += "\\n"; break;
            case '\r': out += "\\r"; break;
            case '\t': out += "\\t"; break;
            default:
                if (static_cast<unsigned char>(c) < 0x20) {
                    char buf[8];
                    std::snprintf(buf, sizeof(buf), "\\u%04x", c);
                    out += buf;
                } else {
                    out.push_back(c);
                }
        }
    }
    out += "\"";
    return out;
}

} // namespace detail

/// The SDK version string.
inline std::string version() {
    char* raw = ilink_version();
    if (raw == nullptr) throw Error("unknown", "ilink_version returned NULL");
    std::string value(raw);
    ilink_string_free(raw);
    return value;
}

/// Run one operation. Returns the `result` JSON; throws [`Error`] on failure.
inline std::string call(const std::string& op, const std::string& params_json = "{}") {
    char* raw = ilink_call(op.c_str(), params_json.empty() ? nullptr : params_json.c_str());
    if (raw == nullptr) throw Error("unknown", "ilink_call returned NULL");
    const std::string envelope(raw);
    ilink_string_free(raw);

    const detail::Field ok_field = detail::top_level_field(envelope, "ok");
    const bool ok = ok_field.found &&
                    envelope.compare(ok_field.begin, ok_field.end - ok_field.begin, "true") == 0;
    if (ok) {
        const detail::Field result = detail::top_level_field(envelope, "result");
        if (!result.found) return "null";
        return envelope.substr(result.begin, result.end - result.begin);
    }

    const detail::Field error_field = detail::top_level_field(envelope, "error");
    const detail::Field kind_field = detail::top_level_field(envelope, "kind");
    const std::string message =
        error_field.found
            ? detail::decode_json_string(envelope.substr(error_field.begin, error_field.end - error_field.begin))
            : "operation failed";
    const std::string kind =
        kind_field.found
            ? detail::decode_json_string(envelope.substr(kind_field.begin, kind_field.end - kind_field.begin))
            : "unknown";
    throw Error(kind, message);
}

/// Run one operation and parse the `result` into a JSON value.
inline json::Value call_json(const std::string& op, const std::string& params_json = "{}") {
    return json::parse(call(op, params_json));
}

/// A client that carries default routing parameters and returns typed models.
///
/// Every helper decodes the operation result into one of the structs in
/// `ilink_types.hpp`. `call` / `call_json` remain available for operations
/// without a dedicated helper.
class Client {
public:
    explicit Client(std::string account_id = "", std::string base_url = "",
                    std::string cdn_base_url = "", std::string route_tag = "",
                    std::string bot_agent = "", int timeout_ms = 0)
        : account_id_(std::move(account_id)),
          base_url_(std::move(base_url)),
          cdn_base_url_(std::move(cdn_base_url)),
          route_tag_(std::move(route_tag)),
          bot_agent_(std::move(bot_agent)),
          timeout_ms_(timeout_ms) {}

    /// Run any operation with the routing defaults merged in (raw result JSON).
    std::string call(const std::string& op, const std::string& params_json = "{}") const {
        return ilink::call(op, merge_routing(params_json));
    }

    /// Run any operation with the routing defaults merged in (parsed result).
    json::Value call_json(const std::string& op, const json::Value& params = json::Value::object()) const {
        return ilink::call_json(op, merge_routing(params.dump()));
    }

    // -- meta -------------------------------------------------------------
    std::string version() const { return ilink::version(); }

    Paths paths() const { return decode<Paths>("paths"); }

    // -- accounts ---------------------------------------------------------
    std::vector<Account> accounts() const {
        std::vector<Account> out;
        const json::Value result = call_json("accounts.list");
        for (std::size_t i = 0; i < result.size(); ++i) {
            out.push_back(from_json(result[i], static_cast<Account*>(nullptr)));
        }
        return out;
    }

    Account resolve_account(const std::string& account_id) const {
        return decode<Account>("account.resolve", obj({{"account_id", account_id}}));
    }

    Account save_account(const std::string& account_id, const std::string& token = "",
                         const std::string& base_url = "", const std::string& user_id = "") const {
        json::Value params = obj({{"account_id", account_id}});
        if (!token.empty()) params.set("token", json::Value(token));
        if (!base_url.empty()) params.set("base_url", json::Value(base_url));
        if (!user_id.empty()) params.set("user_id", json::Value(user_id));
        return decode<Account>("account.save", params);
    }

    void clear_account(const std::string& account_id) const {
        call_json("account.clear", obj({{"account_id", account_id}}));
    }

    // -- QR login ---------------------------------------------------------
    LoginStart login_start(const std::string& base_url = "") const {
        json::Value params = json::Value::object();
        if (!base_url.empty()) params.set("base_url", json::Value(base_url));
        return decode<LoginStart>("login.start", params);
    }

    LoginPoll login_poll(const std::string& session_key) const {
        return decode<LoginPoll>("login.poll", obj({{"session_key", session_key}}));
    }

    void login_submit_verify_code(const std::string& session_key, const std::string& code) const {
        call_json("login.submit_verify_code", obj({{"session_key", session_key}, {"code", code}}));
    }

    LoginStart login_refresh(const std::string& session_key) const {
        return decode<LoginStart>("login.refresh", obj({{"session_key", session_key}}));
    }

    LoginResult login_wait(const std::string& session_key, int timeout_secs = 480,
                           const std::string& verify_code = "") const {
        return decode<LoginResult>("login.wait", login_params(session_key, timeout_secs, verify_code));
    }

    LoginResult login_complete(const std::string& session_key, int timeout_secs = 480,
                               const std::string& verify_code = "") const {
        return decode<LoginResult>("login.complete", login_params(session_key, timeout_secs, verify_code));
    }

    // -- outbound ---------------------------------------------------------
    SendResult send_text(const std::string& to, const std::string& text,
                         const std::string& context_token = "") const {
        json::Value params = obj({{"to", to}, {"text", text}});
        if (!context_token.empty()) params.set("context_token", json::Value(context_token));
        return decode<SendResult>("send.text", params);
    }

    SendResult send_file(const std::string& to, const std::string& path,
                         const std::string& caption = "") const {
        json::Value params = obj({{"to", to}, {"path", path}});
        if (!caption.empty()) params.set("caption", json::Value(caption));
        return decode<SendResult>("send.file", params);
    }

    SendResult send_bytes(const std::string& to, const std::string& file_name,
                          const std::string& data_base64, const std::string& caption = "") const {
        json::Value params = obj({{"to", to}, {"file_name", file_name}, {"data_base64", data_base64}});
        if (!caption.empty()) params.set("caption", json::Value(caption));
        return decode<SendResult>("send.bytes", params);
    }

    SendResult send_item(const std::string& to, const json::Value& item,
                         const std::string& context_token = "") const {
        json::Value params = obj({{"to", to}, {"item", item}});
        if (!context_token.empty()) params.set("context_token", json::Value(context_token));
        return decode<SendResult>("send.item", params);
    }

    // -- inbound / API ----------------------------------------------------
    Updates get_updates(const std::string& get_updates_buf = "", int timeout_ms = 0) const {
        json::Value params = json::Value::object();
        if (!get_updates_buf.empty()) params.set("get_updates_buf", json::Value(get_updates_buf));
        if (timeout_ms > 0) params.set("timeout_ms", json::Value(static_cast<std::int64_t>(timeout_ms)));
        return decode<Updates>("get_updates", params);
    }

    GetConfig get_config(const std::string& user_id, const std::string& context_token = "") const {
        json::Value params = obj({{"user_id", user_id}});
        if (!context_token.empty()) params.set("context_token", json::Value(context_token));
        return decode<GetConfig>("get_config", params);
    }

    void send_typing(const std::string& user_id, const std::string& typing_ticket,
                     const std::string& status = "typing") const {
        call_json("send_typing", obj({{"user_id", user_id},
                                       {"typing_ticket", typing_ticket},
                                       {"status", status}}));
    }

    void notify(const std::string& action = "start") const {
        call_json("notify", obj({{"action", action}}));
    }

    MsgContext context_to_message(const json::Value& message,
                                  const std::string& account_id = "") const {
        json::Value params = obj({{"message", message}});
        if (!account_id.empty()) params.set("account_id", json::Value(account_id));
        return decode<MsgContext>("context.to_message", params);
    }

    std::string message_body(const json::Value& items) const {
        return call_json("message.body", obj({{"items", items}}))["body"].as_string();
    }

    // -- media ------------------------------------------------------------
    UploadedFile upload(const std::string& to, const std::string& path,
                        const std::string& media_type = "file") const {
        return decode<UploadedFile>(
            "media.upload", obj({{"to", to}, {"path", path}, {"media_type", media_type}}));
    }

    MediaFetch download(const json::Value& media, const std::string& aes_key = "",
                        const std::string& dest_path = "") const {
        json::Value params = obj({{"media", media}});
        if (!aes_key.empty()) params.set("aes_key", json::Value(aes_key));
        if (!dest_path.empty()) params.set("dest_path", json::Value(dest_path));
        return decode<MediaFetch>("media.download", params);
    }

    MediaFetch fetch(const std::string& url, const std::string& dest_path = "") const {
        json::Value params = obj({{"url", url}});
        if (!dest_path.empty()) params.set("dest_path", json::Value(dest_path));
        return decode<MediaFetch>("media.fetch", params);
    }

    InboundMedia download_item(const json::Value& item, const std::string& dest_dir = "") const {
        json::Value params = obj({{"item", item}});
        if (!dest_dir.empty()) params.set("dest_dir", json::Value(dest_dir));
        return decode<InboundMedia>("media.download_item", params);
    }

    // -- quote cache ------------------------------------------------------
    QuotePolicy quote_policy() const { return decode<QuotePolicy>("quote.policy"); }

    std::optional<QuoteRecord> quote_find(const std::string& account_id,
                                          const std::string& conversation_id,
                                          const std::string& message_id) const {
        const json::Value result = call_json(
            "quote.find",
            obj({{"account_id", account_id},
                 {"conversation_id", conversation_id},
                 {"message_id", message_id}}));
        if (!result.is_object()) return std::nullopt;
        return from_json(result, static_cast<QuoteRecord*>(nullptr));
    }

    void quote_put(const QuoteRecord& record) const {
        json::Value params = obj({{"account_id", record.account_id},
                                  {"conversation_id", record.conversation_id},
                                  {"message_id", record.message_id},
                                  {"body", record.body}});
        if (!record.direction.empty()) params.set("direction", json::Value(record.direction));
        if (record.media_path) params.set("source_media_path", json::Value(*record.media_path));
        if (record.media_mime) params.set("media_mime", json::Value(*record.media_mime));
        if (record.media_name) params.set("media_name", json::Value(*record.media_name));
        if (record.created_at != 0) params.set("created_at", json::Value(record.created_at));
        call_json("quote.put", params);
    }

    void quote_gc() const { call_json("quote.gc"); }

    void quote_delete_account(const std::string& account_id) const {
        call_json("quote.delete_account", obj({{"account_id", account_id}}));
    }

    std::string quote_media_dir(const std::string& account_id) const {
        return call_json("quote.media_dir", obj({{"account_id", account_id}}))["path"].as_string();
    }

    // -- context tokens ---------------------------------------------------
    std::optional<std::string> context_get(const std::string& account_id,
                                           const std::string& user_id) const {
        const json::Value result =
            call_json("context.get", obj({{"account_id", account_id}, {"user_id", user_id}}));
        if (!result.is_object()) return std::nullopt;
        return result["token"].as_string();
    }

    void context_set(const std::string& account_id, const std::string& user_id,
                     const std::string& token) const {
        call_json("context.set",
                  obj({{"account_id", account_id}, {"user_id", user_id}, {"token", token}}));
    }

    void context_clear(const std::string& account_id) const {
        call_json("context.clear", obj({{"account_id", account_id}}));
    }

    std::int64_t context_restore(const std::string& account_id) const {
        return call_json("context.restore", obj({{"account_id", account_id}}))["restored"].as_int();
    }

    // -- sync cursor ------------------------------------------------------
    std::string sync_load(const std::string& account_id) const {
        return call_json("sync.load", obj({{"account_id", account_id}}))["get_updates_buf"].as_string();
    }

    void sync_save(const std::string& account_id, const std::string& buf) const {
        call_json("sync.save", obj({{"account_id", account_id}, {"buf", buf}}));
    }

    void sync_clear(const std::string& account_id) const {
        call_json("sync.clear", obj({{"account_id", account_id}}));
    }

    // -- session guard ----------------------------------------------------
    SessionStatus session_status(const std::string& account_id = "") const {
        json::Value params = json::Value::object();
        if (!account_id.empty()) params.set("account_id", json::Value(account_id));
        return decode<SessionStatus>("session.status", params);
    }

    void session_pause(const std::string& account_id) const {
        call_json("session.pause", obj({{"account_id", account_id}}));
    }

    void session_resume(const std::string& account_id) const {
        call_json("session.resume", obj({{"account_id", account_id}}));
    }

    void session_assert(const std::string& account_id) const {
        call_json("session.assert", obj({{"account_id", account_id}}));
    }

    // -- helpers ----------------------------------------------------------
    MimeInfo mime_info(const std::string& path) const {
        return decode<MimeInfo>("mime.info", obj({{"path", path}}));
    }

    std::string mime_extension(const std::string& mime) const {
        return call_json("mime.extension", obj({{"mime", mime}}))["extension"].as_string();
    }

    std::string generate_id(const std::string& prefix = "ilink-sdk") const {
        return call_json("util.generate_id", obj({{"prefix", prefix}}))["id"].as_string();
    }

    std::string file_key() const {
        return call_json("util.file_key")["file_key"].as_string();
    }

    std::string temp_name(const std::string& prefix = "ilink-sdk", const std::string& ext = "") const {
        return call_json("util.temp_name", obj({{"prefix", prefix}, {"ext", ext}}))["name"].as_string();
    }

    ConfigLoad config_load() const { return decode<ConfigLoad>("config.load"); }

    // -- crypto -----------------------------------------------------------
    CryptoResult crypto_encrypt(const std::string& key_hex, const std::string& data_base64) const {
        return decode<CryptoResult>(
            "crypto.encrypt", obj({{"key_hex", key_hex}, {"data_base64", data_base64}}));
    }

    std::string crypto_decrypt(const std::string& key_hex, const std::string& data_base64) const {
        return call_json("crypto.decrypt",
                         obj({{"key_hex", key_hex}, {"data_base64", data_base64}}))
            ["data_base64"]
                .as_string();
    }

private:
    static json::Value obj(std::initializer_list<std::pair<std::string, json::Value>> entries) {
        json::Value out = json::Value::object();
        for (const auto& entry : entries) out.set(entry.first, entry.second);
        return out;
    }

    json::Value login_params(const std::string& session_key, int timeout_secs,
                             const std::string& verify_code) const {
        json::Value params = obj({{"session_key", session_key}});
        params.set("timeout_secs", json::Value(static_cast<std::int64_t>(timeout_secs)));
        if (!verify_code.empty()) params.set("verify_code", json::Value(verify_code));
        return params;
    }

    template <typename T>
    T decode(const std::string& op, const json::Value& params = json::Value::object()) const {
        return from_json(call_json(op, params), static_cast<T*>(nullptr));
    }

    std::string merge_routing(const std::string& params_json) const {
        std::string extra;
        auto add = [&](const char* key, const std::string& value) {
            if (value.empty()) return;
            if (!extra.empty()) extra += ",";
            extra += "\"" + std::string(key) + "\":" + detail::quote(value);
        };
        add("account_id", account_id_);
        add("base_url", base_url_);
        add("cdn_base_url", cdn_base_url_);
        add("route_tag", route_tag_);
        add("bot_agent", bot_agent_);
        if (timeout_ms_ > 0) {
            if (!extra.empty()) extra += ",";
            extra += "\"timeout_ms\":" + std::to_string(timeout_ms_);
        }
        if (extra.empty()) return params_json.empty() ? "{}" : params_json;
        if (params_json.empty() || params_json == "{}") return "{" + extra + "}";
        const std::size_t brace = params_json.find('{');
        if (brace == std::string::npos) return params_json;
        return params_json.substr(0, brace + 1) + extra + "," + params_json.substr(brace + 1);
    }

    std::string account_id_;
    std::string base_url_;
    std::string cdn_base_url_;
    std::string route_tag_;
    std::string bot_agent_;
    int timeout_ms_;
};

} // namespace ilink
