package io.ilink.sdk;

import java.util.ArrayList;
import java.util.List;

/**
 * Typed models for the SDK operation results.
 *
 * <p>Each nested class mirrors one operation result and exposes a
 * {@code fromJson} factory that decodes the wire JSON. Absent fields keep their
 * declared defaults, so a newer server never breaks an older binding.
 *
 * <pre>{@code
 * IlinkClient client = new IlinkClient("cfe9a2017e77-im-bot");
 * IlinkModels.SendResult sent = client.sendText("someone@im.wechat", "hi");
 * System.out.println(sent.messageId);
 * }</pre>
 */
public final class IlinkModels {

    private IlinkModels() {}

    private static String str(JsonValue v) {
        return v.asString(null);
    }

    private static List<String> strings(JsonValue v) {
        List<String> out = new ArrayList<>();
        for (JsonValue item : v.items()) {
            out.add(item.asString(""));
        }
        return out;
    }

    /** Resolved local state locations. */
    public static final class Paths {
        public String stateDir = "";
        public String sdkStateDir = "";
        public String accountsDir = "";
        public String configPath = "";

        public static Paths fromJson(JsonValue v) {
            Paths out = new Paths();
            out.stateDir = v.get("state_dir").asString("");
            out.sdkStateDir = v.get("sdk_state_dir").asString("");
            out.accountsDir = v.get("accounts_dir").asString("");
            out.configPath = v.get("config_path").asString("");
            return out;
        }
    }

    /** A resolved, locally stored account. */
    public static final class Account {
        public String accountId = "";
        public String baseUrl = "";
        public String cdnBaseUrl = "";
        public boolean configured;
        public boolean enabled;
        public String userId;
        public String name;
        public String routeTag;
        public boolean tokenPresent;

        public static Account fromJson(JsonValue v) {
            Account out = new Account();
            out.accountId = v.get("account_id").asString("");
            out.baseUrl = v.get("base_url").asString("");
            out.cdnBaseUrl = v.get("cdn_base_url").asString("");
            out.configured = v.get("configured").asBoolean(false);
            out.enabled = v.get("enabled").asBoolean(false);
            out.userId = str(v.get("user_id"));
            out.name = str(v.get("name"));
            out.routeTag = str(v.get("route_tag"));
            out.tokenPresent = v.get("token_present").asBoolean(false);
            return out;
        }
    }

    /** The credential set produced by a successful QR login. */
    public static final class QrSession {
        public String token = "";
        public String botId = "";
        public String baseUrl = "";
        public String userId = "";

        public static QrSession fromJson(JsonValue v) {
            QrSession out = new QrSession();
            out.token = v.get("token").asString("");
            out.botId = v.get("bot_id").asString("");
            out.baseUrl = v.get("base_url").asString("");
            out.userId = v.get("user_id").asString("");
            return out;
        }
    }

    /** The QR code handed to the user at the beginning of a login. */
    public static final class LoginStart {
        public String qrcode = "";
        public String qrcodeUrl = "";
        public String sessionKey = "";

        public static LoginStart fromJson(JsonValue v) {
            LoginStart out = new LoginStart();
            out.qrcode = v.get("qrcode").asString("");
            out.qrcodeUrl = v.get("qrcode_url").asString("");
            out.sessionKey = v.get("session_key").asString("");
            return out;
        }
    }

    /** A non-blocking status check for a login session. */
    public static final class LoginPoll {
        public String kind = "";
        public Long retry;
        public QrSession session;

        public static LoginPoll fromJson(JsonValue v) {
            LoginPoll out = new LoginPoll();
            out.kind = v.get("kind").asString("");
            JsonValue retry = v.get("retry");
            out.retry = retry.isNull() ? null : retry.asLong(0);
            JsonValue session = v.get("session");
            out.session = session.isObject() ? QrSession.fromJson(session) : null;
            return out;
        }
    }

    /** The outcome of a completed QR login. */
    public static final class LoginResult {
        public boolean connected;
        public boolean alreadyConnected;
        public String message = "";
        public String accountId;
        public QrSession session;

        public static LoginResult fromJson(JsonValue v) {
            LoginResult out = new LoginResult();
            out.connected = v.get("connected").asBoolean(false);
            out.alreadyConnected = v.get("already_connected").asBoolean(false);
            out.message = v.get("message").asString("");
            out.accountId = str(v.get("account_id"));
            JsonValue session = v.get("session");
            out.session = session.isObject() ? QrSession.fromJson(session) : null;
            return out;
        }
    }

    /** Identifies a message accepted by the backend. */
    public static final class SendResult {
        public String messageId = "";
        public String serverMessageId = "";

        public static SendResult fromJson(JsonValue v) {
            SendResult out = new SendResult();
            out.messageId = v.get("message_id").asString("");
            out.serverMessageId = v.get("server_message_id").asString("");
            return out;
        }
    }

    /** The result of {@link IlinkClient#getUpdates}. */
    public static final class Updates {
        public Long ret;
        public Long errcode;
        public String errmsg;
        public List<JsonValue> msgs = new ArrayList<>();
        public String getUpdatesBuf;
        public Long longpollingTimeoutMs;

        public static Updates fromJson(JsonValue v) {
            Updates out = new Updates();
            JsonValue ret = v.get("ret");
            out.ret = ret.isNull() ? null : ret.asLong(0);
            JsonValue errcode = v.get("errcode");
            out.errcode = errcode.isNull() ? null : errcode.asLong(0);
            out.errmsg = str(v.get("errmsg"));
            out.msgs = v.get("msgs").items();
            out.getUpdatesBuf = str(v.get("get_updates_buf"));
            JsonValue timeout = v.get("longpolling_timeout_ms");
            out.longpollingTimeoutMs = timeout.isNull() ? null : timeout.asLong(0);
            return out;
        }

        /** {@code true} when the server reported a business-level failure. */
        public boolean isError() {
            return (ret != null && ret != 0) || (errcode != null && errcode != 0);
        }
    }

    /** Bot configuration, including the typing ticket. */
    public static final class GetConfig {
        public Long ret;
        public String errmsg;
        public String typingTicket;

        public static GetConfig fromJson(JsonValue v) {
            GetConfig out = new GetConfig();
            JsonValue ret = v.get("ret");
            out.ret = ret.isNull() ? null : ret.asLong(0);
            out.errmsg = str(v.get("errmsg"));
            out.typingTicket = str(v.get("typing_ticket"));
            return out;
        }

        public boolean isOk() {
            return ret != null && ret == 0;
        }
    }

    /** One downloaded media reference inside a {@link MsgContext}. */
    public static final class MediaEntry {
        public String path = "";
        public String contentType;
        public String fileName;
        public String messageId;

        public static MediaEntry fromJson(JsonValue v) {
            MediaEntry out = new MediaEntry();
            out.path = v.get("path").asString("");
            out.contentType = str(v.get("content_type"));
            out.fileName = str(v.get("file_name"));
            out.messageId = str(v.get("message_id"));
            return out;
        }
    }

    /** The inbound context handed to the host application. */
    public static final class MsgContext {
        public String body = "";
        public String from = "";
        public String to = "";
        public String accountId = "";
        public String originatingChannel = "";
        public String originatingTo = "";
        public String messageSid = "";
        public String messageSidFull;
        public Long timestamp;
        public String provider = "";
        public String chatType = "";
        public String sessionKey;
        public String contextToken;
        public String mediaPath;
        public String mediaType;
        public List<String> mediaPaths = new ArrayList<>();
        public List<String> mediaTypes = new ArrayList<>();
        public List<MediaEntry> media = new ArrayList<>();
        public List<String> channelPromptContext = new ArrayList<>();
        public String replyToId;
        public String replyToBody;
        public String replyToQuoteText;
        public Boolean replyToIsQuote;

        public static MsgContext fromJson(JsonValue v) {
            MsgContext out = new MsgContext();
            out.body = v.get("body").asString("");
            out.from = v.get("from").asString("");
            out.to = v.get("to").asString("");
            out.accountId = v.get("account_id").asString("");
            out.originatingChannel = v.get("originating_channel").asString("");
            out.originatingTo = v.get("originating_to").asString("");
            out.messageSid = v.get("message_sid").asString("");
            out.messageSidFull = str(v.get("message_sid_full"));
            JsonValue ts = v.get("timestamp");
            out.timestamp = ts.isNull() ? null : ts.asLong(0);
            out.provider = v.get("provider").asString("");
            out.chatType = v.get("chat_type").asString("");
            out.sessionKey = str(v.get("session_key"));
            out.contextToken = str(v.get("context_token"));
            out.mediaPath = str(v.get("media_path"));
            out.mediaType = str(v.get("media_type"));
            out.mediaPaths = strings(v.get("media_paths"));
            out.mediaTypes = strings(v.get("media_types"));
            for (JsonValue entry : v.get("media").items()) {
                out.media.add(MediaEntry.fromJson(entry));
            }
            out.channelPromptContext = strings(v.get("channel_prompt_context"));
            out.replyToId = str(v.get("reply_to_id"));
            out.replyToBody = str(v.get("reply_to_body"));
            out.replyToQuoteText = str(v.get("reply_to_quote_text"));
            JsonValue isQuote = v.get("reply_to_is_quote");
            out.replyToIsQuote = isQuote.isNull() ? null : isQuote.asBoolean(false);
            return out;
        }
    }

    /** The result of {@link IlinkClient#upload}. */
    public static final class UploadedFile {
        public String filekey = "";
        public String downloadParam = "";
        public String aeskeyHex = "";
        public String aesKeyBase64 = "";
        public long fileSize;
        public long fileSizeCiphertext;

        public static UploadedFile fromJson(JsonValue v) {
            UploadedFile out = new UploadedFile();
            out.filekey = v.get("filekey").asString("");
            out.downloadParam = v.get("download_param").asString("");
            out.aeskeyHex = v.get("aeskey_hex").asString("");
            out.aesKeyBase64 = v.get("aes_key_base64").asString("");
            out.fileSize = v.get("file_size").asLong(0);
            out.fileSizeCiphertext = v.get("file_size_ciphertext").asLong(0);
            return out;
        }
    }

    /** The result of {@link IlinkClient#downloadItem}. */
    public static final class InboundMedia {
        public String imagePath;
        public String voicePath;
        public String voiceMime;
        public String filePath;
        public String fileMime;
        public String videoPath;
        public boolean referenced;
        public boolean isEmpty;

        public static InboundMedia fromJson(JsonValue v) {
            InboundMedia out = new InboundMedia();
            out.imagePath = str(v.get("image_path"));
            out.voicePath = str(v.get("voice_path"));
            out.voiceMime = str(v.get("voice_mime"));
            out.filePath = str(v.get("file_path"));
            out.fileMime = str(v.get("file_mime"));
            out.videoPath = str(v.get("video_path"));
            out.referenced = v.get("referenced").asBoolean(false);
            out.isEmpty = v.get("is_empty").asBoolean(false);
            return out;
        }

        /** Every downloaded local path, in a stable order. */
        public List<String> paths() {
            List<String> out = new ArrayList<>();
            for (String path : new String[] {imagePath, voicePath, filePath, videoPath}) {
                if (path != null && !path.isEmpty()) {
                    out.add(path);
                }
            }
            return out;
        }
    }

    /** The result of {@link IlinkClient#fetch} / {@link IlinkClient#download}. */
    public static final class MediaFetch {
        public long bytes;
        public String path;
        public String dataBase64;

        public static MediaFetch fromJson(JsonValue v) {
            MediaFetch out = new MediaFetch();
            out.bytes = v.get("bytes").asLong(0);
            out.path = str(v.get("path"));
            out.dataBase64 = str(v.get("data_base64"));
            return out;
        }

        /** Decode the inline payload when one was returned. */
        public byte[] data() {
            return dataBase64 == null ? null : java.util.Base64.getDecoder().decode(dataBase64);
        }
    }

    /** The effective quote-cache policy. */
    public static final class QuotePolicy {
        public boolean enabled;
        public long retentionMs;
        public long maxMessagesPerAccount;
        public long mediaRetentionMs;
        public long maxMediaBytesPerAccount;
        public long maxSingleMediaBytes;

        public static QuotePolicy fromJson(JsonValue v) {
            QuotePolicy out = new QuotePolicy();
            out.enabled = v.get("enabled").asBoolean(false);
            out.retentionMs = v.get("retention_ms").asLong(0);
            out.maxMessagesPerAccount = v.get("max_messages_per_account").asLong(0);
            out.mediaRetentionMs = v.get("media_retention_ms").asLong(0);
            out.maxMediaBytesPerAccount = v.get("max_media_bytes_per_account").asLong(0);
            out.maxSingleMediaBytes = v.get("max_single_media_bytes").asLong(0);
            return out;
        }
    }

    /** One cached quoted message. */
    public static final class QuoteRecord {
        public String accountId = "";
        public String conversationId = "";
        public String messageId = "";
        public String direction = "inbound";
        public String body = "";
        public String mediaPath;
        public String mediaMime;
        public String mediaName;
        public Long mediaSize;
        public long createdAt;

        public static QuoteRecord fromJson(JsonValue v) {
            QuoteRecord out = new QuoteRecord();
            out.accountId = v.get("account_id").asString("");
            out.conversationId = v.get("conversation_id").asString("");
            out.messageId = v.get("message_id").asString("");
            out.direction = v.get("direction").asString("inbound");
            out.body = v.get("body").asString("");
            out.mediaPath = str(v.get("media_path"));
            out.mediaMime = str(v.get("media_mime"));
            out.mediaName = str(v.get("media_name"));
            JsonValue size = v.get("media_size");
            out.mediaSize = size.isNull() ? null : size.asLong(0);
            out.createdAt = v.get("created_at").asLong(0);
            return out;
        }
    }

    /** The session-guard state for an account. */
    public static final class SessionStatus {
        public boolean paused;
        public long remainingMs;

        public static SessionStatus fromJson(JsonValue v) {
            SessionStatus out = new SessionStatus();
            out.paused = v.get("paused").asBoolean(false);
            out.remainingMs = v.get("remaining_ms").asLong(0);
            return out;
        }
    }

    /** Describes a file's media type. */
    public static final class MimeInfo {
        public String mime = "";
        public String kind = "";
        public String extension = "";
        public boolean isImage;
        public boolean isVideo;
        public boolean isAudio;

        public static MimeInfo fromJson(JsonValue v) {
            MimeInfo out = new MimeInfo();
            out.mime = v.get("mime").asString("");
            out.kind = v.get("kind").asString("");
            out.extension = v.get("extension").asString("");
            out.isImage = v.get("is_image").asBoolean(false);
            out.isVideo = v.get("is_video").asBoolean(false);
            out.isAudio = v.get("is_audio").asBoolean(false);
            return out;
        }
    }

    /** The result of {@link IlinkClient#configLoad}. */
    public static final class ConfigLoad {
        public JsonValue config = JsonValue.object();
        public QuotePolicy quoteCachePolicy;

        public static ConfigLoad fromJson(JsonValue v) {
            ConfigLoad out = new ConfigLoad();
            out.config = v.get("config");
            JsonValue policy = v.get("quote_cache_policy");
            out.quoteCachePolicy = policy.isObject() ? QuotePolicy.fromJson(policy) : null;
            return out;
        }
    }

    /** The result of {@link IlinkClient#cryptoEncrypt}. */
    public static final class CryptoResult {
        public String dataBase64 = "";
        public Long paddedSize;

        public static CryptoResult fromJson(JsonValue v) {
            CryptoResult out = new CryptoResult();
            out.dataBase64 = v.get("data_base64").asString("");
            JsonValue size = v.get("padded_size");
            out.paddedSize = size.isNull() ? null : size.asLong(0);
            return out;
        }

        public byte[] data() {
            return java.util.Base64.getDecoder().decode(dataBase64);
        }
    }
}
