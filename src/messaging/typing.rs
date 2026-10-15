//! Typing status, mirroring `src/messaging/typing.ts`.

use crate::api::types::TypingStatus;
use crate::api::WeixinApiClient;
use crate::error::Result;

/// `1` = typing, `2` = cancel typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typing {
    /// The bot is composing.
    Typing,
    /// Stop showing the typing indicator.
    Cancel,
}

impl From<Typing> for TypingStatus {
    fn from(value: Typing) -> Self {
        match value {
            Typing::Typing => TypingStatus::Typing,
            Typing::Cancel => TypingStatus::Cancel,
        }
    }
}

/// Set or cancel the typing indicator for a user.
///
/// # Parameters
/// * `api` — a client whose base URL and token belong to the bot account.
/// * `ilink_user_id` — the recipient's Weixin id.
/// * `ticket` — the ticket from `getConfig`.
/// * `status` — [`Typing::Typing`] or [`Typing::Cancel`].
///
/// An empty ticket is a no-op: `getConfig` only issues one for accounts that
/// support typing, and sending an empty ticket would be rejected.
pub async fn send_typing(
    api: &WeixinApiClient,
    ilink_user_id: &str,
    ticket: &str,
    status: Typing,
) -> Result<()> {
    if ticket.trim().is_empty() {
        tracing::debug!("send_typing: no typing ticket, skipping");
        return Ok(());
    }
    api.send_typing(&crate::api::types::SendTypingReq {
        ilink_user_id: Some(ilink_user_id.to_string()),
        typing_ticket: Some(ticket.to_string()),
        status: Some(TypingStatus::from(status).value()),
    })
    .await
}

/// Typing helpers wired to an [`WeixinApiClient`].
pub struct TypingHelper<'a> {
    api: &'a WeixinApiClient,
    user_id: String,
    ticket: String,
}

impl<'a> TypingHelper<'a> {
    /// Create a helper. An empty ticket disables all calls.
    pub fn new(
        api: &'a WeixinApiClient,
        user_id: impl Into<String>,
        ticket: impl Into<String>,
    ) -> Self {
        TypingHelper {
            api,
            user_id: user_id.into(),
            ticket: ticket.into(),
        }
    }

    /// `true` when a usable ticket is present.
    pub fn enabled(&self) -> bool {
        !self.ticket.trim().is_empty()
    }

    /// Show the typing indicator.
    pub async fn start(&self) -> Result<()> {
        send_typing(self.api, &self.user_id, &self.ticket, Typing::Typing).await
    }

    /// Hide the typing indicator.
    pub async fn stop(&self) -> Result<()> {
        send_typing(self.api, &self.user_id, &self.ticket, Typing::Cancel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_maps_to_wire_values() {
        assert_eq!(TypingStatus::from(Typing::Typing), TypingStatus::Typing);
        assert_eq!(TypingStatus::from(Typing::Cancel), TypingStatus::Cancel);
        assert_eq!(TypingStatus::from(Typing::Typing).value(), 1);
        assert_eq!(TypingStatus::from(Typing::Cancel).value(), 2);
    }

    #[tokio::test]
    async fn empty_ticket_skips_network() {
        // A client pointed at an unreachable host must never be contacted,
        // otherwise this test would reach the network.
        let api = crate::api::WeixinApiClient::new(crate::api::WeixinApiOptions::new(
            "https://invalid.invalid",
        ))
        .unwrap();
        send_typing(&api, "u@im.wechat", "", Typing::Typing)
            .await
            .unwrap();
        send_typing(&api, "u@im.wechat", "   ", Typing::Cancel)
            .await
            .unwrap();
    }
}
