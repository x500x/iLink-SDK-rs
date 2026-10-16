package ilink

// Typed models for the SDK operation results.
//
// The wire protocol is JSON; these structs decode it into Go values so callers
// get field access and compile-time checking instead of hand-rolled maps. Every
// field is optional in the protocol, so the zero value means "absent".

import "encoding/json"

// Account is a resolved, locally stored account.
type Account struct {
	AccountID    string  `json:"account_id"`
	BaseURL      string  `json:"base_url"`
	CDNBaseURL   string  `json:"cdn_base_url"`
	Configured   bool    `json:"configured"`
	Enabled      bool    `json:"enabled"`
	UserID       *string `json:"user_id"`
	Name         *string `json:"name"`
	RouteTag     *string `json:"route_tag"`
	TokenPresent bool    `json:"token_present"`
}

// Paths lists the resolved local state locations.
type Paths struct {
	StateDir    string `json:"state_dir"`
	SDKStateDir string `json:"sdk_state_dir"`
	AccountsDir string `json:"accounts_dir"`
	ConfigPath  string `json:"config_path"`
}

// QrSession is the credential set produced by a successful QR login.
type QrSession struct {
	Token   string `json:"token"`
	BotID   string `json:"bot_id"`
	BaseURL string `json:"base_url"`
	UserID  string `json:"user_id"`
}

// LoginStart is the QR code handed to the user at the beginning of a login.
type LoginStart struct {
	QRCode     string `json:"qrcode"`
	QRCodeURL  string `json:"qrcode_url"`
	SessionKey string `json:"session_key"`
}

// LoginPoll is a non-blocking status check for a login session. Kind is one of
// "continue", "needs_verification", "expired", "verify_code_blocked",
// "already_connected" or "done".
type LoginPoll struct {
	Kind    string     `json:"kind"`
	Retry   *int       `json:"retry"`
	Session *QrSession `json:"session"`
}

// LoginResult is the outcome of a completed QR login.
type LoginResult struct {
	Connected        bool       `json:"connected"`
	AlreadyConnected bool       `json:"already_connected"`
	Message          string     `json:"message"`
	AccountID        string     `json:"account_id"`
	Session          *QrSession `json:"session"`
}

// SendResult identifies a message accepted by the backend.
type SendResult struct {
	MessageID       string `json:"message_id"`
	ServerMessageID string `json:"server_message_id"`
}

// TextItem carries the text of a text message.
type TextItem struct {
	Text *string `json:"text"`
}

// CdnMedia references an encrypted blob on the CDN.
type CdnMedia struct {
	EncryptQueryParam *string `json:"encrypt_query_param"`
	AESKey            *string `json:"aes_key"`
	EncryptType       *int64  `json:"encrypt_type"`
	FullURL           *string `json:"full_url"`
}

// ImageItem is an image message payload.
type ImageItem struct {
	Media       *CdnMedia `json:"media"`
	ThumbMedia  *CdnMedia `json:"thumb_media"`
	AESKey      *string   `json:"aeskey"`
	URL         *string   `json:"url"`
	MidSize     *int64    `json:"mid_size"`
	ThumbSize   *int64    `json:"thumb_size"`
	ThumbHeight *int64    `json:"thumb_height"`
	ThumbWidth  *int64    `json:"thumb_width"`
	HDSize      *int64    `json:"hd_size"`
}

// VoiceItem is a voice message payload.
type VoiceItem struct {
	Media         *CdnMedia `json:"media"`
	EncodeType    *int64    `json:"encode_type"`
	BitsPerSample *int64    `json:"bits_per_sample"`
	SampleRate    *int64    `json:"sample_rate"`
	Playtime      *int64    `json:"playtime"`
	Text          *string   `json:"text"`
}

// FileItem is a file attachment payload.
type FileItem struct {
	Media    *CdnMedia `json:"media"`
	FileName *string   `json:"file_name"`
	MD5      *string   `json:"md5"`
	Len      *string   `json:"len"`
}

// VideoItem is a video message payload.
type VideoItem struct {
	Media       *CdnMedia `json:"media"`
	VideoSize   *int64    `json:"video_size"`
	PlayLength  *int64    `json:"play_length"`
	VideoMD5    *string   `json:"video_md5"`
	ThumbMedia  *CdnMedia `json:"thumb_media"`
	ThumbSize   *int64    `json:"thumb_size"`
	ThumbHeight *int64    `json:"thumb_height"`
	ThumbWidth  *int64    `json:"thumb_width"`
}

// PartialText describes a selected substring of a quoted message.
type PartialText struct {
	Start      string `json:"start"`
	End        string `json:"end"`
	StartIndex int64  `json:"startindex"`
	EndIndex   int64  `json:"endindex"`
	QuoteMD5   string `json:"quotemd5"`
}

// RefMessage is a quoted message attached to an item.
type RefMessage struct {
	MessageItem *MessageItem `json:"message_item"`
	Title       *string      `json:"title"`
	SvrID       *string      `json:"svr_id"`
	PartialText *PartialText `json:"partial_text"`
}

// MessageItem is a single message content item.
type MessageItem struct {
	Type         *int64      `json:"type"`
	MsgID        *string     `json:"msg_id"`
	CreateTimeMS *int64      `json:"create_time_ms"`
	UpdateTimeMS *int64      `json:"update_time_ms"`
	IsCompleted  *bool       `json:"is_completed"`
	RefMsg       *RefMessage `json:"ref_msg"`
	TextItem     *TextItem   `json:"text_item"`
	ImageItem    *ImageItem  `json:"image_item"`
	VoiceItem    *VoiceItem  `json:"voice_item"`
	FileItem     *FileItem   `json:"file_item"`
	VideoItem    *VideoItem  `json:"video_item"`
}

// Text returns the item's text when it is a text item.
func (i MessageItem) Text() (string, bool) {
	if i.TextItem == nil || i.TextItem.Text == nil {
		return "", false
	}
	return *i.TextItem.Text, true
}

// WeixinMessage is a unified inbound or outbound message.
type WeixinMessage struct {
	Seq          *int64        `json:"seq"`
	MessageID    *string       `json:"message_id"`
	FromUserID   *string       `json:"from_user_id"`
	ToUserID     *string       `json:"to_user_id"`
	ClientID     *string       `json:"client_id"`
	CreateTimeMS *int64        `json:"create_time_ms"`
	UpdateTimeMS *int64        `json:"update_time_ms"`
	DeleteTimeMS *int64        `json:"delete_time_ms"`
	SessionID    *string       `json:"session_id"`
	GroupID      *string       `json:"group_id"`
	MessageType  *int64        `json:"message_type"`
	MessageState *int64        `json:"message_state"`
	ItemList     []MessageItem `json:"item_list"`
	ContextToken *string       `json:"context_token"`
	RunID        *string       `json:"run_id"`
}

// Updates is the result of Client.GetUpdates.
type Updates struct {
	Ret                  *int64          `json:"ret"`
	ErrCode              *int64          `json:"errcode"`
	ErrMsg               *string         `json:"errmsg"`
	Msgs                 []WeixinMessage `json:"msgs"`
	GetUpdatesBuf        *string         `json:"get_updates_buf"`
	LongPollingTimeoutMS *int64          `json:"longpolling_timeout_ms"`
}

// Messages is an alias for Msgs.
func (u Updates) Messages() []WeixinMessage { return u.Msgs }

// IsError reports whether the server returned a business-level failure.
func (u Updates) IsError() bool {
	return (u.Ret != nil && *u.Ret != 0) || (u.ErrCode != nil && *u.ErrCode != 0)
}

// GetConfig is the bot configuration returned by Client.GetConfig.
type GetConfig struct {
	Ret          *int64  `json:"ret"`
	ErrMsg       *string `json:"errmsg"`
	TypingTicket *string `json:"typing_ticket"`
}

// IsOK reports whether the config call succeeded (ret == 0).
func (c GetConfig) IsOK() bool { return c.Ret != nil && *c.Ret == 0 }

// MediaEntry is one downloaded media reference inside a MsgContext.
type MediaEntry struct {
	Path        string  `json:"path"`
	ContentType *string `json:"content_type"`
	FileName    *string `json:"file_name"`
	MessageID   *string `json:"message_id"`
}

// MsgContext is the inbound context handed to the host application.
type MsgContext struct {
	Body                 string       `json:"body"`
	From                 string       `json:"from"`
	To                   string       `json:"to"`
	AccountID            string       `json:"account_id"`
	OriginatingChannel   string       `json:"originating_channel"`
	OriginatingTo        string       `json:"originating_to"`
	MessageSID           string       `json:"message_sid"`
	MessageSIDFull       *string      `json:"message_sid_full"`
	Timestamp            *int64       `json:"timestamp"`
	Provider             string       `json:"provider"`
	ChatType             string       `json:"chat_type"`
	SessionKey           *string      `json:"session_key"`
	ContextToken         *string      `json:"context_token"`
	MediaPath            *string      `json:"media_path"`
	MediaType            *string      `json:"media_type"`
	MediaPaths           []string     `json:"media_paths"`
	MediaTypes           []string     `json:"media_types"`
	Media                []MediaEntry `json:"media"`
	ChannelPromptContext []string     `json:"channel_prompt_context"`
	ReplyToID            *string      `json:"reply_to_id"`
	ReplyToBody          *string      `json:"reply_to_body"`
	ReplyToQuoteText     *string      `json:"reply_to_quote_text"`
	ReplyToIsQuote       *bool        `json:"reply_to_is_quote"`
}

// UploadedFile is the result of Client.Upload.
type UploadedFile struct {
	FileKey            string `json:"filekey"`
	DownloadParam      string `json:"download_param"`
	AESKeyHex          string `json:"aeskey_hex"`
	AESKeyBase64       string `json:"aes_key_base64"`
	FileSize           uint64 `json:"file_size"`
	FileSizeCiphertext uint64 `json:"file_size_ciphertext"`
}

// InboundMedia is the result of Client.DownloadItem.
type InboundMedia struct {
	ImagePath  *string `json:"image_path"`
	VoicePath  *string `json:"voice_path"`
	VoiceMime  *string `json:"voice_mime"`
	FilePath   *string `json:"file_path"`
	FileMime   *string `json:"file_mime"`
	VideoPath  *string `json:"video_path"`
	Referenced bool    `json:"referenced"`
	IsEmpty    bool    `json:"is_empty"`
}

// Paths returns every downloaded local path, in a stable order.
func (m InboundMedia) Paths() []string {
	var out []string
	for _, p := range []*string{m.ImagePath, m.VoicePath, m.FilePath, m.VideoPath} {
		if p != nil && *p != "" {
			out = append(out, *p)
		}
	}
	return out
}

// MediaFetch is the result of Client.Fetch / Client.Download.
type MediaFetch struct {
	Bytes      int     `json:"bytes"`
	Path       *string `json:"path"`
	DataBase64 *string `json:"data_base64"`
}

// Data decodes the inline payload when one was returned.
func (m MediaFetch) Data() ([]byte, error) {
	if m.DataBase64 == nil {
		return nil, nil
	}
	return base64Decode(*m.DataBase64)
}

// QuotePolicy is the effective quote-cache policy.
type QuotePolicy struct {
	Enabled                 bool  `json:"enabled"`
	RetentionMS             int64 `json:"retention_ms"`
	MaxMessagesPerAccount   int64 `json:"max_messages_per_account"`
	MediaRetentionMS        int64 `json:"media_retention_ms"`
	MaxMediaBytesPerAccount int64 `json:"max_media_bytes_per_account"`
	MaxSingleMediaBytes     int64 `json:"max_single_media_bytes"`
}

// QuoteRecord is one cached quoted message.
type QuoteRecord struct {
	AccountID      string  `json:"account_id"`
	ConversationID string  `json:"conversation_id"`
	MessageID      string  `json:"message_id"`
	Direction      string  `json:"direction"`
	Body           string  `json:"body"`
	MediaPath      *string `json:"media_path"`
	MediaMime      *string `json:"media_mime"`
	MediaName      *string `json:"media_name"`
	MediaSize      *int64  `json:"media_size"`
	CreatedAt      int64   `json:"created_at"`
}

// SessionStatus is the session-guard state for an account.
type SessionStatus struct {
	Paused      bool  `json:"paused"`
	RemainingMS int64 `json:"remaining_ms"`
}

// MimeInfo describes a file's media type.
type MimeInfo struct {
	Mime      string `json:"mime"`
	Kind      string `json:"kind"`
	Extension string `json:"extension"`
	IsImage   bool   `json:"is_image"`
	IsVideo   bool   `json:"is_video"`
	IsAudio   bool   `json:"is_audio"`
}

// ConfigLoad is the result of Client.ConfigLoad.
type ConfigLoad struct {
	Config           json.RawMessage `json:"config"`
	QuoteCachePolicy *QuotePolicy    `json:"quote_cache_policy"`
}

// CryptoResult is the result of Client.CryptoEncrypt.
type CryptoResult struct {
	DataBase64 string `json:"data_base64"`
	PaddedSize *int64 `json:"padded_size"`
}

// Data decodes the ciphertext.
func (c CryptoResult) Data() ([]byte, error) { return base64Decode(c.DataBase64) }
