// ilink_types.hpp — typed models for the iLink C++ binding.
//
// Each struct mirrors one SDK operation result. `from_json` decodes the wire
// JSON into the struct; absent fields keep their declared default. Callers use
// these through `ilink::Client`, or decode a raw `result` themselves with
// `ilink::json::parse` + the `from_json` overloads.

#pragma once

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

#include "ilink_json.hpp"

namespace ilink {

using json::Value;

// -- small helpers ----------------------------------------------------------

inline std::optional<std::string> opt_string(const Value& v) {
    if (v.is_string()) return v.as_string();
    return std::nullopt;
}

inline std::optional<std::int64_t> opt_int(const Value& v) {
    if (v.is_number()) return v.as_int();
    return std::nullopt;
}

inline std::optional<bool> opt_bool(const Value& v) {
    if (v.is_bool()) return v.as_bool();
    return std::nullopt;
}

// -- meta -------------------------------------------------------------------

struct Paths {
    std::string state_dir;
    std::string sdk_state_dir;
    std::string accounts_dir;
    std::string config_path;
};

inline Paths from_json(const Value& v, Paths*) {
    Paths out;
    out.state_dir = v["state_dir"].as_string();
    out.sdk_state_dir = v["sdk_state_dir"].as_string();
    out.accounts_dir = v["accounts_dir"].as_string();
    out.config_path = v["config_path"].as_string();
    return out;
}

// -- accounts ---------------------------------------------------------------

struct Account {
    std::string account_id;
    std::string base_url;
    std::string cdn_base_url;
    bool configured = false;
    bool enabled = false;
    std::optional<std::string> user_id;
    std::optional<std::string> name;
    std::optional<std::string> route_tag;
    bool token_present = false;
};

inline Account from_json(const Value& v, Account*) {
    Account out;
    out.account_id = v["account_id"].as_string();
    out.base_url = v["base_url"].as_string();
    out.cdn_base_url = v["cdn_base_url"].as_string();
    out.configured = v["configured"].as_bool();
    out.enabled = v["enabled"].as_bool();
    out.user_id = opt_string(v["user_id"]);
    out.name = opt_string(v["name"]);
    out.route_tag = opt_string(v["route_tag"]);
    out.token_present = v["token_present"].as_bool();
    return out;
}

struct QrSession {
    std::string token;
    std::string bot_id;
    std::string base_url;
    std::string user_id;
};

inline QrSession from_json(const Value& v, QrSession*) {
    QrSession out;
    out.token = v["token"].as_string();
    out.bot_id = v["bot_id"].as_string();
    out.base_url = v["base_url"].as_string();
    out.user_id = v["user_id"].as_string();
    return out;
}

struct LoginStart {
    std::string qrcode;
    std::string qrcode_url;
    std::string session_key;
};

inline LoginStart from_json(const Value& v, LoginStart*) {
    LoginStart out;
    out.qrcode = v["qrcode"].as_string();
    out.qrcode_url = v["qrcode_url"].as_string();
    out.session_key = v["session_key"].as_string();
    return out;
}

struct LoginPoll {
    std::string kind;
    std::optional<std::int64_t> retry;
    std::optional<QrSession> session;
};

inline LoginPoll from_json(const Value& v, LoginPoll*) {
    LoginPoll out;
    out.kind = v["kind"].as_string();
    out.retry = opt_int(v["retry"]);
    if (v["session"].is_object()) out.session = from_json(v["session"], static_cast<QrSession*>(nullptr));
    return out;
}

struct LoginResult {
    bool connected = false;
    bool already_connected = false;
    std::string message;
    std::optional<std::string> account_id;
    std::optional<QrSession> session;
};

inline LoginResult from_json(const Value& v, LoginResult*) {
    LoginResult out;
    out.connected = v["connected"].as_bool();
    out.already_connected = v["already_connected"].as_bool();
    out.message = v["message"].as_string();
    out.account_id = opt_string(v["account_id"]);
    if (v["session"].is_object()) out.session = from_json(v["session"], static_cast<QrSession*>(nullptr));
    return out;
}

// -- outbound ---------------------------------------------------------------

struct SendResult {
    std::string message_id;
    std::string server_message_id;
};

inline SendResult from_json(const Value& v, SendResult*) {
    SendResult out;
    out.message_id = v["message_id"].as_string();
    out.server_message_id = v["server_message_id"].as_string();
    return out;
}

// -- inbound / API ----------------------------------------------------------

struct Updates {
    std::optional<std::int64_t> ret;
    std::optional<std::int64_t> errcode;
    std::optional<std::string> errmsg;
    std::vector<Value> msgs;
    std::optional<std::string> get_updates_buf;
    std::optional<std::int64_t> longpolling_timeout_ms;

    /// `true` when the server reported a business-level failure.
    bool is_error() const {
        return (ret && *ret != 0) || (errcode && *errcode != 0);
    }
};

inline Updates from_json(const Value& v, Updates*) {
    Updates out;
    out.ret = opt_int(v["ret"]);
    out.errcode = opt_int(v["errcode"]);
    out.errmsg = opt_string(v["errmsg"]);
    const Value& msgs = v["msgs"];
    if (msgs.is_array()) {
        for (std::size_t i = 0; i < msgs.size(); ++i) out.msgs.push_back(msgs[i]);
    }
    out.get_updates_buf = opt_string(v["get_updates_buf"]);
    out.longpolling_timeout_ms = opt_int(v["longpolling_timeout_ms"]);
    return out;
}

struct GetConfig {
    std::optional<std::int64_t> ret;
    std::optional<std::string> errmsg;
    std::optional<std::string> typing_ticket;

    bool is_ok() const { return ret && *ret == 0; }
};

inline GetConfig from_json(const Value& v, GetConfig*) {
    GetConfig out;
    out.ret = opt_int(v["ret"]);
    out.errmsg = opt_string(v["errmsg"]);
    out.typing_ticket = opt_string(v["typing_ticket"]);
    return out;
}

struct MediaEntry {
    std::string path;
    std::optional<std::string> content_type;
    std::optional<std::string> file_name;
    std::optional<std::string> message_id;
};

inline MediaEntry from_json(const Value& v, MediaEntry*) {
    MediaEntry out;
    out.path = v["path"].as_string();
    out.content_type = opt_string(v["content_type"]);
    out.file_name = opt_string(v["file_name"]);
    out.message_id = opt_string(v["message_id"]);
    return out;
}

struct MsgContext {
    std::string body;
    std::string from;
    std::string to;
    std::string account_id;
    std::string originating_channel;
    std::string originating_to;
    std::string message_sid;
    std::optional<std::string> message_sid_full;
    std::optional<std::int64_t> timestamp;
    std::string provider;
    std::string chat_type;
    std::optional<std::string> session_key;
    std::optional<std::string> context_token;
    std::optional<std::string> media_path;
    std::optional<std::string> media_type;
    std::vector<std::string> media_paths;
    std::vector<std::string> media_types;
    std::vector<MediaEntry> media;
    std::vector<std::string> channel_prompt_context;
    std::optional<std::string> reply_to_id;
    std::optional<std::string> reply_to_body;
    std::optional<std::string> reply_to_quote_text;
    std::optional<bool> reply_to_is_quote;
};

inline MsgContext from_json(const Value& v, MsgContext*) {
    MsgContext out;
    out.body = v["body"].as_string();
    out.from = v["from"].as_string();
    out.to = v["to"].as_string();
    out.account_id = v["account_id"].as_string();
    out.originating_channel = v["originating_channel"].as_string();
    out.originating_to = v["originating_to"].as_string();
    out.message_sid = v["message_sid"].as_string();
    out.message_sid_full = opt_string(v["message_sid_full"]);
    out.timestamp = opt_int(v["timestamp"]);
    out.provider = v["provider"].as_string();
    out.chat_type = v["chat_type"].as_string();
    out.session_key = opt_string(v["session_key"]);
    out.context_token = opt_string(v["context_token"]);
    out.media_path = opt_string(v["media_path"]);
    out.media_type = opt_string(v["media_type"]);
    for (const char* key : {"media_paths", "media_types", "channel_prompt_context"}) {
        const Value& list = v[key];
        if (!list.is_array()) continue;
        std::vector<std::string>* target =
            std::string(key) == "media_paths" ? &out.media_paths
            : std::string(key) == "media_types" ? &out.media_types
                                                : &out.channel_prompt_context;
        for (std::size_t i = 0; i < list.size(); ++i) target->push_back(list[i].as_string());
    }
    const Value& media = v["media"];
    if (media.is_array()) {
        for (std::size_t i = 0; i < media.size(); ++i) {
            out.media.push_back(from_json(media[i], static_cast<MediaEntry*>(nullptr)));
        }
    }
    out.reply_to_id = opt_string(v["reply_to_id"]);
    out.reply_to_body = opt_string(v["reply_to_body"]);
    out.reply_to_quote_text = opt_string(v["reply_to_quote_text"]);
    out.reply_to_is_quote = opt_bool(v["reply_to_is_quote"]);
    return out;
}

// -- media ------------------------------------------------------------------

struct UploadedFile {
    std::string filekey;
    std::string download_param;
    std::string aeskey_hex;
    std::string aes_key_base64;
    std::uint64_t file_size = 0;
    std::uint64_t file_size_ciphertext = 0;
};

inline UploadedFile from_json(const Value& v, UploadedFile*) {
    UploadedFile out;
    out.filekey = v["filekey"].as_string();
    out.download_param = v["download_param"].as_string();
    out.aeskey_hex = v["aeskey_hex"].as_string();
    out.aes_key_base64 = v["aes_key_base64"].as_string();
    out.file_size = static_cast<std::uint64_t>(v["file_size"].as_int());
    out.file_size_ciphertext = static_cast<std::uint64_t>(v["file_size_ciphertext"].as_int());
    return out;
}

struct InboundMedia {
    std::optional<std::string> image_path;
    std::optional<std::string> voice_path;
    std::optional<std::string> voice_mime;
    std::optional<std::string> file_path;
    std::optional<std::string> file_mime;
    std::optional<std::string> video_path;
    bool referenced = false;
    bool is_empty = false;

    /// Every downloaded local path, in a stable order.
    std::vector<std::string> paths() const {
        std::vector<std::string> out;
        for (const auto* p : {&image_path, &voice_path, &file_path, &video_path}) {
            if (p->has_value() && !(*p)->empty()) out.push_back(**p);
        }
        return out;
    }
};

inline InboundMedia from_json(const Value& v, InboundMedia*) {
    InboundMedia out;
    out.image_path = opt_string(v["image_path"]);
    out.voice_path = opt_string(v["voice_path"]);
    out.voice_mime = opt_string(v["voice_mime"]);
    out.file_path = opt_string(v["file_path"]);
    out.file_mime = opt_string(v["file_mime"]);
    out.video_path = opt_string(v["video_path"]);
    out.referenced = v["referenced"].as_bool();
    out.is_empty = v["is_empty"].as_bool();
    return out;
}

struct MediaFetch {
    std::int64_t bytes = 0;
    std::optional<std::string> path;
    std::optional<std::string> data_base64;
};

inline MediaFetch from_json(const Value& v, MediaFetch*) {
    MediaFetch out;
    out.bytes = v["bytes"].as_int();
    out.path = opt_string(v["path"]);
    out.data_base64 = opt_string(v["data_base64"]);
    return out;
}

// -- quote cache ------------------------------------------------------------

struct QuotePolicy {
    bool enabled = false;
    std::uint64_t retention_ms = 0;
    std::uint64_t max_messages_per_account = 0;
    std::uint64_t media_retention_ms = 0;
    std::uint64_t max_media_bytes_per_account = 0;
    std::uint64_t max_single_media_bytes = 0;
};

inline QuotePolicy from_json(const Value& v, QuotePolicy*) {
    QuotePolicy out;
    out.enabled = v["enabled"].as_bool();
    out.retention_ms = static_cast<std::uint64_t>(v["retention_ms"].as_int());
    out.max_messages_per_account = static_cast<std::uint64_t>(v["max_messages_per_account"].as_int());
    out.media_retention_ms = static_cast<std::uint64_t>(v["media_retention_ms"].as_int());
    out.max_media_bytes_per_account = static_cast<std::uint64_t>(v["max_media_bytes_per_account"].as_int());
    out.max_single_media_bytes = static_cast<std::uint64_t>(v["max_single_media_bytes"].as_int());
    return out;
}

struct QuoteRecord {
    std::string account_id;
    std::string conversation_id;
    std::string message_id;
    std::string direction;
    std::string body;
    std::optional<std::string> media_path;
    std::optional<std::string> media_mime;
    std::optional<std::string> media_name;
    std::optional<std::int64_t> media_size;
    std::int64_t created_at = 0;
};

inline QuoteRecord from_json(const Value& v, QuoteRecord*) {
    QuoteRecord out;
    out.account_id = v["account_id"].as_string();
    out.conversation_id = v["conversation_id"].as_string();
    out.message_id = v["message_id"].as_string();
    out.direction = v["direction"].as_string();
    out.body = v["body"].as_string();
    out.media_path = opt_string(v["media_path"]);
    out.media_mime = opt_string(v["media_mime"]);
    out.media_name = opt_string(v["media_name"]);
    out.media_size = opt_int(v["media_size"]);
    out.created_at = v["created_at"].as_int();
    return out;
}

// -- session / helpers ------------------------------------------------------

struct SessionStatus {
    bool paused = false;
    std::int64_t remaining_ms = 0;
};

inline SessionStatus from_json(const Value& v, SessionStatus*) {
    SessionStatus out;
    out.paused = v["paused"].as_bool();
    out.remaining_ms = v["remaining_ms"].as_int();
    return out;
}

struct MimeInfo {
    std::string mime;
    std::string kind;
    std::string extension;
    bool is_image = false;
    bool is_video = false;
    bool is_audio = false;
};

inline MimeInfo from_json(const Value& v, MimeInfo*) {
    MimeInfo out;
    out.mime = v["mime"].as_string();
    out.kind = v["kind"].as_string();
    out.extension = v["extension"].as_string();
    out.is_image = v["is_image"].as_bool();
    out.is_video = v["is_video"].as_bool();
    out.is_audio = v["is_audio"].as_bool();
    return out;
}

struct ConfigLoad {
    Value config;
    std::optional<QuotePolicy> quote_cache_policy;
};

inline ConfigLoad from_json(const Value& v, ConfigLoad*) {
    ConfigLoad out;
    out.config = v["config"];
    if (v["quote_cache_policy"].is_object()) {
        out.quote_cache_policy = from_json(v["quote_cache_policy"], static_cast<QuotePolicy*>(nullptr));
    }
    return out;
}

struct CryptoResult {
    std::string data_base64;
    std::optional<std::int64_t> padded_size;
};

inline CryptoResult from_json(const Value& v, CryptoResult*) {
    CryptoResult out;
    out.data_base64 = v["data_base64"].as_string();
    out.padded_size = opt_int(v["padded_size"]);
    return out;
}

} // namespace ilink
