package io.ilink.sdk

/**
 * Typed models for the SDK operation results.
 *
 * Each class mirrors one operation result and exposes a `fromJson` factory that
 * decodes the wire JSON. Absent fields keep their declared defaults, so a newer
 * server never breaks an older binding.
 *
 * ```kotlin
 * val client = IlinkClient(accountId = "cfe9a2017e77-im-bot")
 * val sent: SendResult = client.sendText("someone@im.wechat", "hi")
 * println(sent.messageId)
 * ```
 */
object IlinkModels {

    private fun str(v: JsonValue): String? = if (v.isNull) null else v.asString()

    private fun strings(v: JsonValue): List<String> = v.items.map { it.asString() }

    /** Resolved local state locations. */
    data class Paths(
        val stateDir: String = "",
        val sdkStateDir: String = "",
        val accountsDir: String = "",
        val configPath: String = "",
    ) {
        companion object {
            fun fromJson(v: JsonValue) = Paths(
                stateDir = v["state_dir"].asString(),
                sdkStateDir = v["sdk_state_dir"].asString(),
                accountsDir = v["accounts_dir"].asString(),
                configPath = v["config_path"].asString(),
            )
        }
    }

    /** A resolved, locally stored account. */
    data class Account(
        val accountId: String = "",
        val baseUrl: String = "",
        val cdnBaseUrl: String = "",
        val configured: Boolean = false,
        val enabled: Boolean = false,
        val userId: String? = null,
        val name: String? = null,
        val routeTag: String? = null,
        val tokenPresent: Boolean = false,
    ) {
        companion object {
            fun fromJson(v: JsonValue) = Account(
                accountId = v["account_id"].asString(),
                baseUrl = v["base_url"].asString(),
                cdnBaseUrl = v["cdn_base_url"].asString(),
                configured = v["configured"].asBoolean(),
                enabled = v["enabled"].asBoolean(),
                userId = str(v["user_id"]),
                name = str(v["name"]),
                routeTag = str(v["route_tag"]),
                tokenPresent = v["token_present"].asBoolean(),
            )
        }
    }

    /** The credential set produced by a successful QR login. */
    data class QrSession(
        val token: String = "",
        val botId: String = "",
        val baseUrl: String = "",
        val userId: String = "",
    ) {
        companion object {
            fun fromJson(v: JsonValue) = QrSession(
                token = v["token"].asString(),
                botId = v["bot_id"].asString(),
                baseUrl = v["base_url"].asString(),
                userId = v["user_id"].asString(),
            )
        }
    }

    /** The QR code handed to the user at the beginning of a login. */
    data class LoginStart(
        val qrcode: String = "",
        val qrcodeUrl: String = "",
        val sessionKey: String = "",
    ) {
        companion object {
            fun fromJson(v: JsonValue) = LoginStart(
                qrcode = v["qrcode"].asString(),
                qrcodeUrl = v["qrcode_url"].asString(),
                sessionKey = v["session_key"].asString(),
            )
        }
    }

    /** A non-blocking status check for a login session. */
    data class LoginPoll(
        val kind: String = "",
        val retry: Long? = null,
        val session: QrSession? = null,
    ) {
        companion object {
            fun fromJson(v: JsonValue): LoginPoll {
                val retry = v["retry"]
                val session = v["session"]
                return LoginPoll(
                    kind = v["kind"].asString(),
                    retry = if (retry.isNull) null else retry.asLong(),
                    session = if (session.isObject) QrSession.fromJson(session) else null,
                )
            }
        }
    }

    /** The outcome of a completed QR login. */
    data class LoginResult(
        val connected: Boolean = false,
        val alreadyConnected: Boolean = false,
        val message: String = "",
        val accountId: String? = null,
        val session: QrSession? = null,
    ) {
        companion object {
            fun fromJson(v: JsonValue): LoginResult {
                val session = v["session"]
                return LoginResult(
                    connected = v["connected"].asBoolean(),
                    alreadyConnected = v["already_connected"].asBoolean(),
                    message = v["message"].asString(),
                    accountId = str(v["account_id"]),
                    session = if (session.isObject) QrSession.fromJson(session) else null,
                )
            }
        }
    }

    /** Identifies a message accepted by the backend. */
    data class SendResult(
        val messageId: String = "",
        val serverMessageId: String = "",
    ) {
        companion object {
            fun fromJson(v: JsonValue) = SendResult(
                messageId = v["message_id"].asString(),
                serverMessageId = v["server_message_id"].asString(),
            )
        }
    }

    /** The result of [IlinkClient.getUpdates]. */
    data class Updates(
        val ret: Long? = null,
        val errcode: Long? = null,
        val errmsg: String? = null,
        val msgs: List<JsonValue> = emptyList(),
        val getUpdatesBuf: String? = null,
        val longpollingTimeoutMs: Long? = null,
    ) {
        /** `true` when the server reported a business-level failure. */
        val isError: Boolean
            get() = (ret != null && ret != 0L) || (errcode != null && errcode != 0L)

        companion object {
            fun fromJson(v: JsonValue): Updates {
                val ret = v["ret"]
                val errcode = v["errcode"]
                val timeout = v["longpolling_timeout_ms"]
                return Updates(
                    ret = if (ret.isNull) null else ret.asLong(),
                    errcode = if (errcode.isNull) null else errcode.asLong(),
                    errmsg = str(v["errmsg"]),
                    msgs = v["msgs"].items,
                    getUpdatesBuf = str(v["get_updates_buf"]),
                    longpollingTimeoutMs = if (timeout.isNull) null else timeout.asLong(),
                )
            }
        }
    }

    /** Bot configuration, including the typing ticket. */
    data class GetConfig(
        val ret: Long? = null,
        val errmsg: String? = null,
        val typingTicket: String? = null,
    ) {
        val isOk: Boolean get() = ret != null && ret == 0L

        companion object {
            fun fromJson(v: JsonValue): GetConfig {
                val ret = v["ret"]
                return GetConfig(
                    ret = if (ret.isNull) null else ret.asLong(),
                    errmsg = str(v["errmsg"]),
                    typingTicket = str(v["typing_ticket"]),
                )
            }
        }
    }

    /** One downloaded media reference inside a [MsgContext]. */
    data class MediaEntry(
        val path: String = "",
        val contentType: String? = null,
        val fileName: String? = null,
        val messageId: String? = null,
    ) {
        companion object {
            fun fromJson(v: JsonValue) = MediaEntry(
                path = v["path"].asString(),
                contentType = str(v["content_type"]),
                fileName = str(v["file_name"]),
                messageId = str(v["message_id"]),
            )
        }
    }

    /** The inbound context handed to the host application. */
    data class MsgContext(
        val body: String = "",
        val from: String = "",
        val to: String = "",
        val accountId: String = "",
        val originatingChannel: String = "",
        val originatingTo: String = "",
        val messageSid: String = "",
        val messageSidFull: String? = null,
        val timestamp: Long? = null,
        val provider: String = "",
        val chatType: String = "",
        val sessionKey: String? = null,
        val contextToken: String? = null,
        val mediaPath: String? = null,
        val mediaType: String? = null,
        val mediaPaths: List<String> = emptyList(),
        val mediaTypes: List<String> = emptyList(),
        val media: List<MediaEntry> = emptyList(),
        val channelPromptContext: List<String> = emptyList(),
        val replyToId: String? = null,
        val replyToBody: String? = null,
        val replyToQuoteText: String? = null,
        val replyToIsQuote: Boolean? = null,
    ) {
        companion object {
            fun fromJson(v: JsonValue): MsgContext {
                val timestamp = v["timestamp"]
                val isQuote = v["reply_to_is_quote"]
                return MsgContext(
                    body = v["body"].asString(),
                    from = v["from"].asString(),
                    to = v["to"].asString(),
                    accountId = v["account_id"].asString(),
                    originatingChannel = v["originating_channel"].asString(),
                    originatingTo = v["originating_to"].asString(),
                    messageSid = v["message_sid"].asString(),
                    messageSidFull = str(v["message_sid_full"]),
                    timestamp = if (timestamp.isNull) null else timestamp.asLong(),
                    provider = v["provider"].asString(),
                    chatType = v["chat_type"].asString(),
                    sessionKey = str(v["session_key"]),
                    contextToken = str(v["context_token"]),
                    mediaPath = str(v["media_path"]),
                    mediaType = str(v["media_type"]),
                    mediaPaths = strings(v["media_paths"]),
                    mediaTypes = strings(v["media_types"]),
                    media = v["media"].items.map { MediaEntry.fromJson(it) },
                    channelPromptContext = strings(v["channel_prompt_context"]),
                    replyToId = str(v["reply_to_id"]),
                    replyToBody = str(v["reply_to_body"]),
                    replyToQuoteText = str(v["reply_to_quote_text"]),
                    replyToIsQuote = if (isQuote.isNull) null else isQuote.asBoolean(),
                )
            }
        }
    }

    /** The result of [IlinkClient.upload]. */
    data class UploadedFile(
        val filekey: String = "",
        val downloadParam: String = "",
        val aeskeyHex: String = "",
        val aesKeyBase64: String = "",
        val fileSize: Long = 0,
        val fileSizeCiphertext: Long = 0,
    ) {
        companion object {
            fun fromJson(v: JsonValue) = UploadedFile(
                filekey = v["filekey"].asString(),
                downloadParam = v["download_param"].asString(),
                aeskeyHex = v["aeskey_hex"].asString(),
                aesKeyBase64 = v["aes_key_base64"].asString(),
                fileSize = v["file_size"].asLong(),
                fileSizeCiphertext = v["file_size_ciphertext"].asLong(),
            )
        }
    }

    /** The result of [IlinkClient.downloadItem]. */
    data class InboundMedia(
        val imagePath: String? = null,
        val voicePath: String? = null,
        val voiceMime: String? = null,
        val filePath: String? = null,
        val fileMime: String? = null,
        val videoPath: String? = null,
        val referenced: Boolean = false,
        val isEmpty: Boolean = false,
    ) {
        /** Every downloaded local path, in a stable order. */
        val paths: List<String>
            get() = listOfNotNull(imagePath, voicePath, filePath, videoPath).filter { it.isNotEmpty() }

        companion object {
            fun fromJson(v: JsonValue) = InboundMedia(
                imagePath = str(v["image_path"]),
                voicePath = str(v["voice_path"]),
                voiceMime = str(v["voice_mime"]),
                filePath = str(v["file_path"]),
                fileMime = str(v["file_mime"]),
                videoPath = str(v["video_path"]),
                referenced = v["referenced"].asBoolean(),
                isEmpty = v["is_empty"].asBoolean(),
            )
        }
    }

    /** The result of [IlinkClient.fetch] / [IlinkClient.download]. */
    data class MediaFetch(
        val bytes: Long = 0,
        val path: String? = null,
        val dataBase64: String? = null,
    ) {
        /** Decode the inline payload when one was returned. */
        fun data(): ByteArray? = dataBase64?.let { java.util.Base64.getDecoder().decode(it) }

        companion object {
            fun fromJson(v: JsonValue) = MediaFetch(
                bytes = v["bytes"].asLong(),
                path = str(v["path"]),
                dataBase64 = str(v["data_base64"]),
            )
        }
    }

    /** The effective quote-cache policy. */
    data class QuotePolicy(
        val enabled: Boolean = false,
        val retentionMs: Long = 0,
        val maxMessagesPerAccount: Long = 0,
        val mediaRetentionMs: Long = 0,
        val maxMediaBytesPerAccount: Long = 0,
        val maxSingleMediaBytes: Long = 0,
    ) {
        companion object {
            fun fromJson(v: JsonValue) = QuotePolicy(
                enabled = v["enabled"].asBoolean(),
                retentionMs = v["retention_ms"].asLong(),
                maxMessagesPerAccount = v["max_messages_per_account"].asLong(),
                mediaRetentionMs = v["media_retention_ms"].asLong(),
                maxMediaBytesPerAccount = v["max_media_bytes_per_account"].asLong(),
                maxSingleMediaBytes = v["max_single_media_bytes"].asLong(),
            )
        }
    }

    /** One cached quoted message. */
    data class QuoteRecord(
        val accountId: String = "",
        val conversationId: String = "",
        val messageId: String = "",
        val direction: String = "inbound",
        val body: String = "",
        val mediaPath: String? = null,
        val mediaMime: String? = null,
        val mediaName: String? = null,
        val mediaSize: Long? = null,
        val createdAt: Long = 0,
    ) {
        companion object {
            fun fromJson(v: JsonValue): QuoteRecord {
                val size = v["media_size"]
                return QuoteRecord(
                    accountId = v["account_id"].asString(),
                    conversationId = v["conversation_id"].asString(),
                    messageId = v["message_id"].asString(),
                    direction = v["direction"].asString("inbound"),
                    body = v["body"].asString(),
                    mediaPath = str(v["media_path"]),
                    mediaMime = str(v["media_mime"]),
                    mediaName = str(v["media_name"]),
                    mediaSize = if (size.isNull) null else size.asLong(),
                    createdAt = v["created_at"].asLong(),
                )
            }
        }
    }

    /** The session-guard state for an account. */
    data class SessionStatus(
        val paused: Boolean = false,
        val remainingMs: Long = 0,
    ) {
        companion object {
            fun fromJson(v: JsonValue) = SessionStatus(
                paused = v["paused"].asBoolean(),
                remainingMs = v["remaining_ms"].asLong(),
            )
        }
    }

    /** Describes a file's media type. */
    data class MimeInfo(
        val mime: String = "",
        val kind: String = "",
        val extension: String = "",
        val isImage: Boolean = false,
        val isVideo: Boolean = false,
        val isAudio: Boolean = false,
    ) {
        companion object {
            fun fromJson(v: JsonValue) = MimeInfo(
                mime = v["mime"].asString(),
                kind = v["kind"].asString(),
                extension = v["extension"].asString(),
                isImage = v["is_image"].asBoolean(),
                isVideo = v["is_video"].asBoolean(),
                isAudio = v["is_audio"].asBoolean(),
            )
        }
    }

    /** The result of [IlinkClient.configLoad]. */
    data class ConfigLoad(
        val config: JsonValue = JsonValue.objectValue(),
        val quoteCachePolicy: QuotePolicy? = null,
    ) {
        companion object {
            fun fromJson(v: JsonValue): ConfigLoad {
                val policy = v["quote_cache_policy"]
                return ConfigLoad(
                    config = v["config"],
                    quoteCachePolicy = if (policy.isObject) QuotePolicy.fromJson(policy) else null,
                )
            }
        }
    }

    /** The result of [IlinkClient.cryptoEncrypt]. */
    data class CryptoResult(
        val dataBase64: String = "",
        val paddedSize: Long? = null,
    ) {
        fun data(): ByteArray = java.util.Base64.getDecoder().decode(dataBase64)

        companion object {
            fun fromJson(v: JsonValue): CryptoResult {
                val size = v["padded_size"]
                return CryptoResult(
                    dataBase64 = v["data_base64"].asString(),
                    paddedSize = if (size.isNull) null else size.asLong(),
                )
            }
        }
    }
}
