//! End-to-end tests over a mock HTTP server.
//!
//! These exercise the real HTTP path (headers, JSON bodies, timeout, lossless
//! uid decoding) without touching the Weixin backend.

use std::time::Duration;

use ilink_sdk::api::types::{GetUpdatesReq, GetUpdatesResp, SendMessageReq, WeixinMessage};
use ilink_sdk::api::{WeixinApiClient, WeixinApiOptions, DEFAULT_LONG_POLL_TIMEOUT_MS};
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn client_for(server: &MockServer) -> WeixinApiClient {
    let opts = WeixinApiOptions::new(server.uri()).with_token("test-bot-token");
    WeixinApiClient::new(opts).expect("client")
}

#[tokio::test]
async fn get_updates_sends_cursor_and_returns_msgs() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/ilink/bot/getupdates"))
        .and(header("authorization", "Bearer test-bot-token"))
        .and(header("authorizationtype", "ilink_bot_token"))
        .and(header("ilink-app-id", "bot"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ret": 0,
            "msgs": [{
                "message_id": 18446744073709551615u64,
                "from_user_id": "u@im.wechat",
                "item_list": [{"type": 1, "text_item": {"text": "hi"}}],
                "context_token": "ct-1"
            }],
            "get_updates_buf": "cursor-2",
            "longpolling_timeout_ms": 20000
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let resp = client
        .get_updates(
            &GetUpdatesReq {
                get_updates_buf: Some("cursor-1".into()),
                sync_buf: None,
            },
            Some(5_000),
            None,
        )
        .await
        .expect("get_updates");

    assert_eq!(resp.ret, Some(0));
    assert_eq!(resp.get_updates_buf.as_deref(), Some("cursor-2"));
    // The uint64 must survive losslessly — a plain u64 parse would overflow
    // or clamp this value.
    let msg = &resp.msgs.as_ref().unwrap()[0];
    assert_eq!(msg.message_id.as_deref(), Some("18446744073709551615"));
    assert_eq!(resp.longpolling_timeout_ms, Some(20000));
}

#[tokio::test]
async fn get_updates_empty_on_timeout() {
    let server = MockServer::start().await;
    // Never respond; the client-side timeout must produce an empty result
    // carrying the same cursor, exactly like the TypeScript behaviour.
    Mock::given(method("POST"))
        .and(path("/ilink/bot/getupdates"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let resp = client
        .get_updates(
            &GetUpdatesReq {
                get_updates_buf: Some("keep-me".into()),
                sync_buf: None,
            },
            Some(300),
            None,
        )
        .await
        .expect("timeout must not be an error");

    assert_eq!(resp.ret, Some(0));
    assert!(resp.msgs.as_ref().is_none_or(|m| m.is_empty()));
    assert_eq!(resp.get_updates_buf.as_deref(), Some("keep-me"));
}

#[tokio::test]
async fn get_updates_aborts_when_cancelled() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/ilink/bot/getupdates"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let token = tokio_util::sync::CancellationToken::new();
    let cancel = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        cancel.cancel();
    });

    let resp = client
        .get_updates(
            &GetUpdatesReq {
                get_updates_buf: Some("c".into()),
                sync_buf: None,
            },
            Some(DEFAULT_LONG_POLL_TIMEOUT_MS),
            Some(token),
        )
        .await
        .expect("cancellation returns an empty response, not an error");
    assert_eq!(resp.get_updates_buf.as_deref(), Some("c"));
}

#[tokio::test]
async fn send_message_posts_full_body_and_returns_server_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/ilink/bot/sendmessage"))
        .and(body_partial_json(serde_json::json!({
            "msg": {
                "to_user_id": "u@im.wechat",
                "message_type": 2,
                "message_state": 2,
                "context_token": "ct-9",
                "item_list": [{"type": 1, "text_item": {"text": "hello"}}]
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message_id": 4242,
            "ret": 0,
            "errmsg": ""
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let req = SendMessageReq {
        msg: Some(WeixinMessage {
            to_user_id: Some("u@im.wechat".into()),
            message_type: Some(2),
            message_state: Some(2),
            context_token: Some("ct-9".into()),
            item_list: Some(vec![ilink_sdk::api::types::MessageItem {
                r#type: Some(1),
                text_item: Some(ilink_sdk::api::types::TextItem {
                    text: Some("hello".into()),
                }),
                ..Default::default()
            }]),
            ..Default::default()
        }),
    };
    let resp = client.send_message(&req).await.expect("send");
    assert_eq!(resp.message_id.as_deref(), Some("4242"));
    assert!(!resp.is_error());
}

#[tokio::test]
async fn send_message_non_zero_ret_is_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/ilink/bot/sendmessage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ret": 7,
            "errmsg": "blocked"
        })))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let err = client
        .send_message(&SendMessageReq::default())
        .await
        .expect_err("ret=7 must be an error");
    assert!(err.to_string().contains("ret=7"), "got {err}");
}

#[tokio::test]
async fn http_500_is_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/ilink/bot/getupdates"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let err = client
        .get_updates(
            &GetUpdatesReq {
                get_updates_buf: None,
                sync_buf: None,
            },
            Some(5_000),
            None,
        )
        .await
        .expect_err("500 must fail");
    assert!(err.to_string().contains("500"), "got {err}");
}

#[tokio::test]
async fn get_upload_url_posts_all_fields() {
    use ilink_sdk::api::types::GetUploadUrlReq;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/ilink/bot/getuploadurl"))
        .and(body_partial_json(serde_json::json!({
            "filekey": "k1",
            "media_type": 1,
            "to_user_id": "u@im.wechat",
            "rawsize": 11,
            "no_need_thumb": true,
            "aeskey": "00112233445566778899aabbccddeeff"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "upload_param": "p",
            "thumb_upload_param": "",
            "upload_full_url": ""
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let resp = client
        .get_upload_url(&GetUploadUrlReq {
            filekey: Some("k1".into()),
            media_type: Some(1),
            to_user_id: Some("u@im.wechat".into()),
            rawsize: Some(11),
            rawfilemd5: Some("md5".into()),
            filesize: Some(16),
            no_need_thumb: Some(true),
            aeskey: Some("00112233445566778899aabbccddeeff".into()),
            ..Default::default()
        })
        .await
        .expect("upload url");
    assert_eq!(resp.upload_param.as_deref(), Some("p"));
}

#[tokio::test]
async fn get_config_requires_ret_zero() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/ilink/bot/getconfig"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ret": 0,
            "typing_ticket": "ticket"
        })))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let resp = client
        .get_config("u@im.wechat", Some("ct"))
        .await
        .expect("config");
    assert!(resp.is_ok());
    assert_eq!(resp.typing_ticket.as_deref(), Some("ticket"));
}

#[tokio::test]
async fn notify_start_and_stop_hit_their_paths() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/ilink/bot/msg/notifystart"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ret": 0})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/ilink/bot/msg/notifystop"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ret": 0})))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    client.notify_start().await.expect("start");
    client.notify_stop().await.expect("stop");
}

#[tokio::test]
async fn connection_refused_is_a_transport_error() {
    // Bind then drop a listener so the port is closed but known to be unused.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let client =
        WeixinApiClient::new(WeixinApiOptions::new(format!("http://127.0.0.1:{port}"))).unwrap();

    // No client-side timeout, so the only outcome is the OS refusal itself.
    let err = client
        .get_updates(
            &GetUpdatesReq {
                get_updates_buf: Some("keep".into()),
                sync_buf: None,
            },
            None,
            None,
        )
        .await
        .expect_err("must fail to connect");

    eprintln!("kind={:?} msg={err}", err.kind());
    // A refusal is not a cancellation, so the monitor loop keeps retrying
    // rather than treating it as a clean stop.
    assert!(!err.is_abort(), "a refusal is not a cancellation");

    // The client-side timeout path is separate: with a 1ms budget the request
    // must be aborted and turned into an empty result carrying the cursor.
    let resp = client
        .get_updates(
            &GetUpdatesReq {
                get_updates_buf: Some("keep".into()),
                sync_buf: None,
            },
            Some(1),
            None,
        )
        .await
        .expect("timeout returns an empty response, not an error");
    assert_eq!(resp.get_updates_buf.as_deref(), Some("keep"));
    assert_eq!(resp.ret, Some(0));
}

#[tokio::test]
async fn parse_api_json_keeps_uint64_in_nested_message() {
    let client = WeixinApiClient::new(WeixinApiOptions::new("http://127.0.0.1:1")).unwrap();
    let raw = r#"{"ret":0,"msgs":[{"message_id":18446744073709551615}]}"#;
    let parsed: GetUpdatesResp = ilink_sdk::api::parse_api_json(raw).unwrap();
    let msg = &parsed.msgs.unwrap()[0];
    assert_eq!(msg.message_id.as_deref(), Some("18446744073709551615"));
    let _ = client;
}
