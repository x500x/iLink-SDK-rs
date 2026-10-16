package ilink

// The ergonomic, typed client. Every method decodes the operation result into
// one of the models in types.go, so callers never touch raw JSON for the common
// flows. Call / CallInto remain available for operations without a dedicated
// helper.

import (
	"encoding/base64"
	"encoding/json"
)

func base64Decode(raw string) ([]byte, error) { return base64.StdEncoding.DecodeString(raw) }

// Config holds the default routing parameters applied to every call.
type Config struct {
	AccountID  string
	BaseURL    string
	CDNBaseURL string
	RouteTag   string
	BotAgent   string
	TimeoutMS  int
}

// Client is an ergonomic wrapper that merges Config into every operation.
type Client struct {
	cfg Config
}

// NewClient returns a Client with the given default routing parameters.
func NewClient(cfg Config) *Client { return &Client{cfg: cfg} }

// Call runs any operation with the client's routing parameters merged in,
// returning the raw `result` JSON.
func (c *Client) Call(op string, params map[string]any) (json.RawMessage, error) {
	merged := map[string]any{}
	if c.cfg.AccountID != "" {
		merged["account_id"] = c.cfg.AccountID
	}
	if c.cfg.BaseURL != "" {
		merged["base_url"] = c.cfg.BaseURL
	}
	if c.cfg.CDNBaseURL != "" {
		merged["cdn_base_url"] = c.cfg.CDNBaseURL
	}
	if c.cfg.RouteTag != "" {
		merged["route_tag"] = c.cfg.RouteTag
	}
	if c.cfg.BotAgent != "" {
		merged["bot_agent"] = c.cfg.BotAgent
	}
	if c.cfg.TimeoutMS > 0 {
		merged["timeout_ms"] = c.cfg.TimeoutMS
	}
	for key, value := range params {
		merged[key] = value
	}
	return Call(op, merged)
}

// CallInto runs an operation and decodes the result into out.
func (c *Client) CallInto(op string, params map[string]any, out any) error {
	raw, err := c.Call(op, params)
	if err != nil {
		return err
	}
	return json.Unmarshal(raw, out)
}

// Version returns the SDK version string.
func (c *Client) Version() (string, error) { return Version() }

// Paths lists the resolved local state locations.
func (c *Client) Paths() (*Paths, error) {
	var out Paths
	err := c.CallInto("paths", nil, &out)
	return &out, err
}

// -- accounts ---------------------------------------------------------------

// Accounts lists the locally registered accounts.
func (c *Client) Accounts() ([]Account, error) {
	var accounts []Account
	err := c.CallInto("accounts.list", nil, &accounts)
	return accounts, err
}

// ResolveAccount returns the resolved account for id.
func (c *Client) ResolveAccount(id string) (*Account, error) {
	var account Account
	if err := c.CallInto("account.resolve", map[string]any{"account_id": id}, &account); err != nil {
		return nil, err
	}
	return &account, nil
}

// SaveAccount persists credentials for id.
func (c *Client) SaveAccount(id, token, baseURL, userID string) (*Account, error) {
	params := map[string]any{"account_id": id}
	if token != "" {
		params["token"] = token
	}
	if baseURL != "" {
		params["base_url"] = baseURL
	}
	if userID != "" {
		params["user_id"] = userID
	}
	var account Account
	if err := c.CallInto("account.save", params, &account); err != nil {
		return nil, err
	}
	return &account, nil
}

// ClearAccount forgets an account and all of its local state.
func (c *Client) ClearAccount(id string) error {
	return c.CallInto("account.clear", map[string]any{"account_id": id}, new(json.RawMessage))
}

// -- QR login ---------------------------------------------------------------

// LoginStart begins a QR login and returns the code to display.
func (c *Client) LoginStart(baseURL string) (*LoginStart, error) {
	params := map[string]any{}
	if baseURL != "" {
		params["base_url"] = baseURL
	}
	var start LoginStart
	if err := c.CallInto("login.start", params, &start); err != nil {
		return nil, err
	}
	return &start, nil
}

// LoginPoll performs a non-blocking status check for a login session.
func (c *Client) LoginPoll(sessionKey string) (*LoginPoll, error) {
	var poll LoginPoll
	if err := c.CallInto("login.poll", map[string]any{"session_key": sessionKey}, &poll); err != nil {
		return nil, err
	}
	return &poll, nil
}

// LoginSubmitVerifyCode answers a verification challenge for a login session.
func (c *Client) LoginSubmitVerifyCode(sessionKey, code string) error {
	return c.CallInto(
		"login.submit_verify_code",
		map[string]any{"session_key": sessionKey, "code": code},
		new(json.RawMessage),
	)
}

// LoginRefresh fetches a fresh QR code for an existing session.
func (c *Client) LoginRefresh(sessionKey string) (*LoginStart, error) {
	var start LoginStart
	if err := c.CallInto("login.refresh", map[string]any{"session_key": sessionKey}, &start); err != nil {
		return nil, err
	}
	return &start, nil
}

// LoginWait waits for the user to confirm, without persisting the credentials.
func (c *Client) LoginWait(sessionKey string, timeoutSecs int, verifyCode string) (*LoginResult, error) {
	params := map[string]any{"session_key": sessionKey}
	if timeoutSecs > 0 {
		params["timeout_secs"] = timeoutSecs
	}
	if verifyCode != "" {
		params["verify_code"] = verifyCode
	}
	var result LoginResult
	if err := c.CallInto("login.wait", params, &result); err != nil {
		return nil, err
	}
	return &result, nil
}

// LoginComplete waits for the user to confirm and persists the credentials.
func (c *Client) LoginComplete(sessionKey string, timeoutSecs int, verifyCode string) (*LoginResult, error) {
	params := map[string]any{"session_key": sessionKey}
	if timeoutSecs > 0 {
		params["timeout_secs"] = timeoutSecs
	}
	if verifyCode != "" {
		params["verify_code"] = verifyCode
	}
	var result LoginResult
	if err := c.CallInto("login.complete", params, &result); err != nil {
		return nil, err
	}
	return &result, nil
}

// -- outbound ---------------------------------------------------------------

// SendText sends a text message.
func (c *Client) SendText(to, text, contextToken, runID string) (*SendResult, error) {
	params := map[string]any{"to": to, "text": text}
	if contextToken != "" {
		params["context_token"] = contextToken
	}
	if runID != "" {
		params["run_id"] = runID
	}
	return c.send("send.text", params)
}

// SendFile uploads and sends a local file (image/video/file by MIME type).
func (c *Client) SendFile(to, path, caption string) (*SendResult, error) {
	params := map[string]any{"to": to, "path": path}
	if caption != "" {
		params["caption"] = caption
	}
	return c.send("send.file", params)
}

// SendBytes sends in-memory bytes; data is base64 encoded for the wire.
func (c *Client) SendBytes(to, fileName string, data []byte, caption string) (*SendResult, error) {
	params := map[string]any{
		"to":          to,
		"file_name":   fileName,
		"data_base64": base64.StdEncoding.EncodeToString(data),
	}
	if caption != "" {
		params["caption"] = caption
	}
	return c.send("send.bytes", params)
}

// SendItem sends a raw protocol item (advanced).
func (c *Client) SendItem(to string, item any, contextToken string) (*SendResult, error) {
	params := map[string]any{"to": to, "item": item}
	if contextToken != "" {
		params["context_token"] = contextToken
	}
	return c.send("send.item", params)
}

func (c *Client) send(op string, params map[string]any) (*SendResult, error) {
	var result SendResult
	if err := c.CallInto(op, params, &result); err != nil {
		return nil, err
	}
	return &result, nil
}

// -- inbound / API ----------------------------------------------------------

// GetUpdates long-polls for inbound messages; pass the previous cursor back in.
func (c *Client) GetUpdates(getUpdatesBuf string, timeoutMS int) (*Updates, error) {
	params := map[string]any{}
	if getUpdatesBuf != "" {
		params["get_updates_buf"] = getUpdatesBuf
	}
	if timeoutMS > 0 {
		params["timeout_ms"] = timeoutMS
	}
	var updates Updates
	if err := c.CallInto("get_updates", params, &updates); err != nil {
		return nil, err
	}
	return &updates, nil
}

// GetConfig returns the bot config, including the typing ticket.
func (c *Client) GetConfig(userID, contextToken string) (*GetConfig, error) {
	params := map[string]any{"user_id": userID}
	if contextToken != "" {
		params["context_token"] = contextToken
	}
	var cfg GetConfig
	if err := c.CallInto("get_config", params, &cfg); err != nil {
		return nil, err
	}
	return &cfg, nil
}

// SendTyping shows or cancels the typing indicator.
func (c *Client) SendTyping(userID, typingTicket, status string) error {
	params := map[string]any{"user_id": userID, "typing_ticket": typingTicket}
	if status != "" {
		params["status"] = status
	}
	return c.CallInto("send_typing", params, new(json.RawMessage))
}

// Notify sends a lifecycle ping ("start" or "stop").
func (c *Client) Notify(action string) error {
	return c.CallInto("notify", map[string]any{"action": action}, new(json.RawMessage))
}

// ContextToMessage parses a raw inbound message into a MsgContext.
func (c *Client) ContextToMessage(message any) (*MsgContext, error) {
	var ctx MsgContext
	if err := c.CallInto("context.to_message", map[string]any{"message": message}, &ctx); err != nil {
		return nil, err
	}
	return &ctx, nil
}

// MessageBody extracts the display body from a list of message items.
func (c *Client) MessageBody(items any) (string, error) {
	var out struct {
		Body string `json:"body"`
	}
	if err := c.CallInto("message.body", map[string]any{"items": items}, &out); err != nil {
		return "", err
	}
	return out.Body, nil
}

// -- media ------------------------------------------------------------------

// Upload uploads a file to the CDN and returns the reference.
func (c *Client) Upload(to, path, mediaType string) (*UploadedFile, error) {
	params := map[string]any{"to": to, "path": path}
	if mediaType != "" {
		params["media_type"] = mediaType
	}
	var uploaded UploadedFile
	if err := c.CallInto("media.upload", params, &uploaded); err != nil {
		return nil, err
	}
	return &uploaded, nil
}

// Download fetches a CDN reference, decrypting it when aesKey is set.
func (c *Client) Download(media any, aesKey, destPath string) (*MediaFetch, error) {
	params := map[string]any{"media": media}
	if aesKey != "" {
		params["aes_key"] = aesKey
	}
	if destPath != "" {
		params["dest_path"] = destPath
	}
	var out MediaFetch
	if err := c.CallInto("media.download", params, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// Fetch downloads a URL, optionally writing it to destPath.
func (c *Client) Fetch(url, destPath string) (*MediaFetch, error) {
	params := map[string]any{"url": url}
	if destPath != "" {
		params["dest_path"] = destPath
	}
	var out MediaFetch
	if err := c.CallInto("media.fetch", params, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// DownloadItem downloads every media referenced by an inbound message item.
func (c *Client) DownloadItem(item any, destDir string) (*InboundMedia, error) {
	params := map[string]any{"item": item}
	if destDir != "" {
		params["dest_dir"] = destDir
	}
	var out InboundMedia
	if err := c.CallInto("media.download_item", params, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// -- quote cache ------------------------------------------------------------

// QuotePolicy returns the effective quote-cache policy.
func (c *Client) QuotePolicy() (*QuotePolicy, error) {
	var out QuotePolicy
	if err := c.CallInto("quote.policy", nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// QuoteFind returns a cached quoted message, or nil when absent.
func (c *Client) QuoteFind(accountID, conversationID, messageID string) (*QuoteRecord, error) {
	raw, err := c.Call("quote.find", map[string]any{
		"account_id":      accountID,
		"conversation_id": conversationID,
		"message_id":      messageID,
	})
	if err != nil {
		return nil, err
	}
	if string(raw) == "null" || len(raw) == 0 {
		return nil, nil
	}
	var record QuoteRecord
	if err := json.Unmarshal(raw, &record); err != nil {
		return nil, err
	}
	return &record, nil
}

// QuotePut stores a message in the quote cache.
func (c *Client) QuotePut(record QuoteRecord) error {
	params := map[string]any{
		"account_id":      record.AccountID,
		"conversation_id": record.ConversationID,
		"message_id":      record.MessageID,
		"body":            record.Body,
	}
	if record.Direction != "" {
		params["direction"] = record.Direction
	}
	if record.MediaPath != nil {
		params["source_media_path"] = *record.MediaPath
	}
	if record.MediaMime != nil {
		params["media_mime"] = *record.MediaMime
	}
	if record.MediaName != nil {
		params["media_name"] = *record.MediaName
	}
	if record.CreatedAt != 0 {
		params["created_at"] = record.CreatedAt
	}
	return c.CallInto("quote.put", params, new(json.RawMessage))
}

// QuoteGC garbage-collects expired cache entries.
func (c *Client) QuoteGC() error {
	return c.CallInto("quote.gc", nil, new(json.RawMessage))
}

// QuoteDeleteAccount removes every cached entry for an account.
func (c *Client) QuoteDeleteAccount(accountID string) error {
	return c.CallInto("quote.delete_account", map[string]any{"account_id": accountID}, new(json.RawMessage))
}

// QuoteMediaDir returns the managed media directory for an account.
func (c *Client) QuoteMediaDir(accountID string) (string, error) {
	var out struct {
		Path string `json:"path"`
	}
	if err := c.CallInto("quote.media_dir", map[string]any{"account_id": accountID}, &out); err != nil {
		return "", err
	}
	return out.Path, nil
}

// -- context tokens ---------------------------------------------------------

// ContextGet returns the stored context token, or "" when absent.
func (c *Client) ContextGet(accountID, userID string) (string, error) {
	raw, err := c.Call("context.get", map[string]any{"account_id": accountID, "user_id": userID})
	if err != nil {
		return "", err
	}
	if string(raw) == "null" || len(raw) == 0 {
		return "", nil
	}
	var out struct {
		Token string `json:"token"`
	}
	if err := json.Unmarshal(raw, &out); err != nil {
		return "", err
	}
	return out.Token, nil
}

// ContextSet stores a context token.
func (c *Client) ContextSet(accountID, userID, token string) error {
	return c.CallInto(
		"context.set",
		map[string]any{"account_id": accountID, "user_id": userID, "token": token},
		new(json.RawMessage),
	)
}

// ContextClear removes every context token for an account.
func (c *Client) ContextClear(accountID string) error {
	return c.CallInto("context.clear", map[string]any{"account_id": accountID}, new(json.RawMessage))
}

// ContextRestore reloads the persisted tokens and returns how many were read.
func (c *Client) ContextRestore(accountID string) (int, error) {
	var out struct {
		Restored int `json:"restored"`
	}
	if err := c.CallInto("context.restore", map[string]any{"account_id": accountID}, &out); err != nil {
		return 0, err
	}
	return out.Restored, nil
}

// -- sync cursor ------------------------------------------------------------

// SyncLoad returns the stored getUpdates cursor.
func (c *Client) SyncLoad(accountID string) (string, error) {
	var out struct {
		GetUpdatesBuf string `json:"get_updates_buf"`
	}
	if err := c.CallInto("sync.load", map[string]any{"account_id": accountID}, &out); err != nil {
		return "", err
	}
	return out.GetUpdatesBuf, nil
}

// SyncSave persists the getUpdates cursor.
func (c *Client) SyncSave(accountID, buf string) error {
	return c.CallInto("sync.save", map[string]any{"account_id": accountID, "buf": buf}, new(json.RawMessage))
}

// SyncClear forgets the getUpdates cursor.
func (c *Client) SyncClear(accountID string) error {
	return c.CallInto("sync.clear", map[string]any{"account_id": accountID}, new(json.RawMessage))
}

// -- session guard ----------------------------------------------------------

// SessionStatus returns the session-guard state for an account.
func (c *Client) SessionStatus(accountID string) (*SessionStatus, error) {
	params := map[string]any{}
	if accountID != "" {
		params["account_id"] = accountID
	}
	var out SessionStatus
	if err := c.CallInto("session.status", params, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// SessionPause pauses message processing for an account.
func (c *Client) SessionPause(accountID string) error {
	return c.CallInto("session.pause", map[string]any{"account_id": accountID}, new(json.RawMessage))
}

// SessionResume resumes message processing for an account.
func (c *Client) SessionResume(accountID string) error {
	return c.CallInto("session.resume", map[string]any{"account_id": accountID}, new(json.RawMessage))
}

// SessionAssert returns an error when the session is paused.
func (c *Client) SessionAssert(accountID string) error {
	return c.CallInto("session.assert", map[string]any{"account_id": accountID}, new(json.RawMessage))
}

// -- helpers ----------------------------------------------------------------

// MimeInfo describes a file's media type.
func (c *Client) MimeInfo(path string) (*MimeInfo, error) {
	var out MimeInfo
	if err := c.CallInto("mime.info", map[string]any{"path": path}, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// MimeExtension returns the extension for a MIME type.
func (c *Client) MimeExtension(mime string) (string, error) {
	var out struct {
		Extension string `json:"extension"`
	}
	if err := c.CallInto("mime.extension", map[string]any{"mime": mime}, &out); err != nil {
		return "", err
	}
	return out.Extension, nil
}

// GenerateID returns a new unique identifier.
func (c *Client) GenerateID(prefix string) (string, error) {
	params := map[string]any{}
	if prefix != "" {
		params["prefix"] = prefix
	}
	var out struct {
		ID string `json:"id"`
	}
	if err := c.CallInto("util.generate_id", params, &out); err != nil {
		return "", err
	}
	return out.ID, nil
}

// TempName returns a fresh temporary file name.
func (c *Client) TempName(prefix, ext string) (string, error) {
	var out struct {
		Name string `json:"name"`
	}
	if err := c.CallInto("util.temp_name", map[string]any{"prefix": prefix, "ext": ext}, &out); err != nil {
		return "", err
	}
	return out.Name, nil
}

// ConfigLoad returns the effective local configuration.
func (c *Client) ConfigLoad() (*ConfigLoad, error) {
	var out ConfigLoad
	if err := c.CallInto("config.load", nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// -- crypto -----------------------------------------------------------------

// CryptoEncrypt AES-128-ECB encrypts data with a 16-byte hex key.
func (c *Client) CryptoEncrypt(keyHex string, data []byte) (*CryptoResult, error) {
	var out CryptoResult
	params := map[string]any{
		"key_hex":     keyHex,
		"data_base64": base64.StdEncoding.EncodeToString(data),
	}
	if err := c.CallInto("crypto.encrypt", params, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// CryptoDecrypt AES-128-ECB decrypts base64 ciphertext with a 16-byte hex key.
func (c *Client) CryptoDecrypt(keyHex, dataBase64 string) ([]byte, error) {
	var out struct {
		DataBase64 string `json:"data_base64"`
	}
	if err := c.CallInto(
		"crypto.decrypt",
		map[string]any{"key_hex": keyHex, "data_base64": dataBase64},
		&out,
	); err != nil {
		return nil, err
	}
	return base64Decode(out.DataBase64)
}
