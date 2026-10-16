// ilink_sdk.hpp — header-only C++ wrapper for the iLink (Weixin) SDK.
//
// Wraps the shared library's JSON C ABI in RAII + exceptions:
//
//     #include "ilink_sdk.hpp"
//     std::string result = ilink::call("mime.info", R"({"path":"a.png"})");
//
// `call` returns the `result` JSON as a string (parse it with your JSON library
// of choice) and throws `ilink::Error` on failure, carrying the Rust `kind`.
//
// Compile with `-I include` and link the shared library, e.g.
//     g++ -std=c++17 -I include app.cpp target/release/ilink_sdk.dll -o app

#pragma once

#include <cstddef>
#include <cstdio>
#include <stdexcept>
#include <string>
#include <utility>

#include "ilink_sdk.h"

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

/// A client that carries default routing parameters for every call.
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

    std::string call(const std::string& op, const std::string& params_json = "{}") const {
        return ilink::call(op, merge_routing(params_json));
    }

    std::string accounts() const { return call("accounts.list"); }

    std::string login_start(const std::string& base_url = "") const {
        return call("login.start", base_url.empty() ? "{}" : "{\"base_url\":" + detail::quote(base_url) + "}");
    }

    std::string login_complete(const std::string& session_key, int timeout_secs = 480,
                               const std::string& verify_code = "") const {
        std::string params = "{\"session_key\":" + detail::quote(session_key) +
                             ",\"timeout_secs\":" + std::to_string(timeout_secs);
        if (!verify_code.empty()) params += ",\"verify_code\":" + detail::quote(verify_code);
        params += "}";
        return call("login.complete", params);
    }

    std::string send_text(const std::string& to, const std::string& text,
                          const std::string& context_token = "") const {
        std::string params = "{\"to\":" + detail::quote(to) + ",\"text\":" + detail::quote(text);
        if (!context_token.empty()) params += ",\"context_token\":" + detail::quote(context_token);
        params += "}";
        return call("send.text", params);
    }

    std::string send_file(const std::string& to, const std::string& path,
                          const std::string& caption = "") const {
        std::string params = "{\"to\":" + detail::quote(to) + ",\"path\":" + detail::quote(path);
        if (!caption.empty()) params += ",\"caption\":" + detail::quote(caption);
        params += "}";
        return call("send.file", params);
    }

    std::string get_updates(const std::string& get_updates_buf = "", int timeout_ms = 0) const {
        std::string params = "{";
        if (!get_updates_buf.empty()) params += "\"get_updates_buf\":" + detail::quote(get_updates_buf);
        if (timeout_ms > 0) {
            if (params.size() > 1) params += ",";
            params += "\"timeout_ms\":" + std::to_string(timeout_ms);
        }
        params += "}";
        return call("get_updates", params);
    }

    std::string send_typing(const std::string& user_id, const std::string& typing_ticket,
                            const std::string& status = "typing") const {
        return call("send_typing", "{\"user_id\":" + detail::quote(user_id) +
                                       ",\"typing_ticket\":" + detail::quote(typing_ticket) +
                                       ",\"status\":" + detail::quote(status) + "}");
    }

    std::string upload(const std::string& to, const std::string& path,
                       const std::string& media_type = "file") const {
        return call("media.upload", "{\"to\":" + detail::quote(to) + ",\"path\":" +
                                        detail::quote(path) + ",\"media_type\":" +
                                        detail::quote(media_type) + "}");
    }

private:
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
