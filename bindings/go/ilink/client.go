package ilink

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

// LoginStart is the QR code handed to the user at the beginning of a login.
type LoginStart struct {
	QRCode     string `json:"qrcode"`
	QRCodeURL  string `json:"qrcode_url"`
	SessionKey string `json:"session_key"`
}

// SendResult identifies a message accepted by the backend.
type SendResult struct {
	MessageID       string `json:"message_id"`
	ServerMessageID string `json:"server_message_id"`
}

// LoginComplete is the outcome of a completed QR login.
type LoginComplete struct {
	Connected        bool            `json:"connected"`
	AlreadyConnected bool            `json:"already_connected"`
	Message          string          `json:"message"`
	AccountID        string          `json:"account_id"`
	Session          json.RawMessage `json:"session"`
}

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

// Call runs any operation with the client's routing parameters merged in.
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

// Accounts lists the locally registered accounts.
func (c *Client) Accounts() ([]Account, error) {
	var accounts []Account
	err := c.CallInto("accounts.list", nil, &accounts)
	return accounts, err
}

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

// LoginComplete waits for the user to confirm and persists the credentials.
func (c *Client) LoginComplete(sessionKey string, timeoutSecs int, verifyCode string) (*LoginComplete, error) {
	params := map[string]any{"session_key": sessionKey}
	if timeoutSecs > 0 {
		params["timeout_secs"] = timeoutSecs
	}
	if verifyCode != "" {
		params["verify_code"] = verifyCode
	}
	var done LoginComplete
	if err := c.CallInto("login.complete", params, &done); err != nil {
		return nil, err
	}
	return &done, nil
}

// SendText sends a text message.
func (c *Client) SendText(to, text, contextToken, runID string) (*SendResult, error) {
	params := map[string]any{"to": to, "text": text}
	if contextToken != "" {
		params["context_token"] = contextToken
	}
	if runID != "" {
		params["run_id"] = runID
	}
	var result SendResult
	if err := c.CallInto("send.text", params, &result); err != nil {
		return nil, err
	}
	return &result, nil
}

// SendFile uploads and sends a local file (image/video/file by MIME type).
func (c *Client) SendFile(to, path, caption string) (*SendResult, error) {
	params := map[string]any{"to": to, "path": path}
	if caption != "" {
		params["caption"] = caption
	}
	var result SendResult
	if err := c.CallInto("send.file", params, &result); err != nil {
		return nil, err
	}
	return &result, nil
}

// GetUpdates long-polls for inbound messages; pass the previous cursor back in.
func (c *Client) GetUpdates(getUpdatesBuf string, timeoutMS int) (json.RawMessage, error) {
	params := map[string]any{}
	if getUpdatesBuf != "" {
		params["get_updates_buf"] = getUpdatesBuf
	}
	if timeoutMS > 0 {
		params["timeout_ms"] = timeoutMS
	}
	return c.Call("get_updates", params)
}

// SendTyping shows or cancels the typing indicator.
func (c *Client) SendTyping(userID, typingTicket, status string) error {
	params := map[string]any{"user_id": userID, "typing_ticket": typingTicket}
	if status != "" {
		params["status"] = status
	}
	return c.CallInto("send_typing", params, new(json.RawMessage))
}

// Upload uploads a file to the CDN and returns the reference JSON.
func (c *Client) Upload(to, path, mediaType string) (json.RawMessage, error) {
	params := map[string]any{"to": to, "path": path}
	if mediaType != "" {
		params["media_type"] = mediaType
	}
	return c.Call("media.upload", params)
}

// CallInto runs an operation and decodes the result into out.
func (c *Client) CallInto(op string, params map[string]any, out any) error {
	raw, err := c.Call(op, params)
	if err != nil {
		return err
	}
	return json.Unmarshal(raw, out)
}
