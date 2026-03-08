use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, ensure};
use futures_util::{SinkExt, StreamExt};
use prost::Message;
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::Deserialize;
use serde_json::Value;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use uuid::Uuid;

use crate::feishu::{
    client::FeishuClient, dispatcher::FeishuDispatcher, secret_store::FeishuSecretStore,
    service::FeishuService,
};

const FEISHU_LONG_CONNECTION_ENDPOINT: &str = "https://open.feishu.cn/callback/ws/endpoint";
const FRAME_METHOD_CONTROL: i32 = 0;
const FRAME_METHOD_DATA: i32 = 1;
const HEADER_TYPE: &str = "type";
const HEADER_MESSAGE_ID: &str = "message_id";
const HEADER_SUM: &str = "sum";
const HEADER_SEQ: &str = "seq";
const HEADER_BIZ_RT: &str = "biz_rt";
const MESSAGE_TYPE_EVENT: &str = "event";
const MESSAGE_TYPE_PING: &str = "ping";
const MESSAGE_TYPE_PONG: &str = "pong";
const CHUNK_CACHE_TTL: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct FeishuLongConnection<S, C>
where
    S: FeishuSecretStore,
    C: FeishuClient,
{
    bot_id: Uuid,
    service: Arc<FeishuService<S>>,
    dispatcher: FeishuDispatcher<S, C>,
    http: Client,
    endpoint: String,
}

impl<S, C> FeishuLongConnection<S, C>
where
    S: FeishuSecretStore + 'static,
    C: FeishuClient + 'static,
{
    pub fn new(
        bot_id: Uuid,
        service: Arc<FeishuService<S>>,
        dispatcher: FeishuDispatcher<S, C>,
    ) -> Self {
        Self {
            bot_id,
            service,
            dispatcher,
            http: Client::new(),
            endpoint: FEISHU_LONG_CONNECTION_ENDPOINT.to_string(),
        }
    }

    #[cfg(test)]
    pub fn with_endpoint(
        bot_id: Uuid,
        service: Arc<FeishuService<S>>,
        dispatcher: FeishuDispatcher<S, C>,
        endpoint: impl Into<String>,
    ) -> Self {
        Self {
            bot_id,
            service,
            dispatcher,
            http: Client::new(),
            endpoint: endpoint.into(),
        }
    }

    pub async fn run_once(&self) -> Result<()> {
        let bot = self
            .service
            .get_bot(self.bot_id)
            .await?
            .context("Feishu bot not found for long connection")?;
        if !bot.enabled {
            return Ok(());
        }

        let secrets = self.service.resolve_bot_secrets(&bot).await?;
        let connect_info = self
            .fetch_connect_info(&bot.app_id, secrets.app_secret.expose_secret())
            .await?;
        let (stream, _response) = connect_async(connect_info.url.as_str())
            .await
            .context("Failed to connect Feishu long connection websocket")?;
        tracing::info!(bot_id = %self.bot_id, "Feishu long connection connected");
        let (mut writer, mut reader) = stream.split();
        let mut cache = MessageChunkCache::default();
        let mut ping_interval = tokio::time::interval(connect_info.ping_interval);
        ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ping_interval.tick().await;

        loop {
            tokio::select! {
                _ = ping_interval.tick() => {
                    send_frame(
                        &mut writer,
                        ProtoFrame {
                            seq_id: 0,
                            log_id: 0,
                            service: connect_info.service_id,
                            method: FRAME_METHOD_CONTROL,
                            headers: vec![ProtoHeader {
                                key: HEADER_TYPE.to_string(),
                                value: MESSAGE_TYPE_PING.to_string(),
                            }],
                            payload_encoding: String::new(),
                            payload_type: String::new(),
                            payload: Vec::new(),
                            log_id_new: String::new(),
                        },
                    )
                    .await?;
                }
                message = reader.next() => {
                    match message {
                        Some(Ok(WsMessage::Binary(bytes))) => {
                            if let Some(updated_ping_interval) = self
                                .handle_binary_frame(&mut writer, &mut cache, bytes.as_ref())
                                .await?
                            {
                                ping_interval = tokio::time::interval(updated_ping_interval);
                                ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                                ping_interval.tick().await;
                            }
                        }
                        Some(Ok(WsMessage::Ping(payload))) => {
                            writer.send(WsMessage::Pong(payload)).await?;
                        }
                        Some(Ok(WsMessage::Pong(_))) => {}
                        Some(Ok(WsMessage::Close(close_frame))) => {
                            return Err(anyhow!("Feishu long connection closed: {:?}", close_frame));
                        }
                        Some(Ok(WsMessage::Text(_))) => {}
                        Some(Ok(WsMessage::Frame(_))) => {}
                        Some(Err(error)) => {
                            return Err(error).context("Feishu long connection websocket receive failed");
                        }
                        None => return Err(anyhow!("Feishu long connection stream ended unexpectedly")),
                    }
                }
            }
        }
    }

    async fn fetch_connect_info(&self, app_id: &str, app_secret: &str) -> Result<ConnectInfo> {
        let response = self
            .http
            .post(&self.endpoint)
            .header("locale", "zh")
            .json(&serde_json::json!({
                "AppID": app_id,
                "AppSecret": app_secret,
            }))
            .send()
            .await
            .context("Failed to request Feishu long connection config")?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        ensure!(
            status.is_success(),
            "Feishu long connection config request failed with status {}: {}",
            status,
            body
        );

        let payload: ConnectConfigResponse = serde_json::from_str(&body)
            .context("Invalid Feishu long connection config response")?;
        ensure!(
            payload.code == 0,
            "Feishu long connection config failed: {}",
            payload.msg
        );

        let data = payload
            .data
            .context("Feishu long connection config response missing data")?;
        let url = data.url;
        Ok(ConnectInfo {
            service_id: extract_service_id(&url)?,
            ping_interval: Duration::from_secs(data.client_config.ping_interval.max(1)),
            url,
        })
    }

    async fn handle_binary_frame<Writer>(
        &self,
        writer: &mut Writer,
        cache: &mut MessageChunkCache,
        bytes: &[u8],
    ) -> Result<Option<Duration>>
    where
        Writer: futures_util::Sink<WsMessage> + Unpin,
        Writer::Error: std::error::Error + Send + Sync + 'static,
    {
        let frame = ProtoFrame::decode(bytes).context("Failed to decode Feishu websocket frame")?;
        match frame.method {
            FRAME_METHOD_CONTROL => handle_control_frame(&frame),
            FRAME_METHOD_DATA => {
                if frame_header_value(&frame, HEADER_TYPE) != Some(MESSAGE_TYPE_EVENT) {
                    return Ok(None);
                }
                let Some(event_payload) = cache.merge(&frame)? else {
                    return Ok(None);
                };

                let started_at = Instant::now();
                let result = self.handle_event_payload(event_payload).await;
                let response_code = if result.is_ok() { 200 } else { 500 };

                send_frame(
                    writer,
                    ProtoFrame {
                        seq_id: frame.seq_id,
                        log_id: frame.log_id,
                        service: frame.service,
                        method: frame.method,
                        headers: frame
                            .headers
                            .iter()
                            .cloned()
                            .chain(std::iter::once(ProtoHeader {
                                key: HEADER_BIZ_RT.to_string(),
                                value: started_at.elapsed().as_millis().to_string(),
                            }))
                            .collect(),
                        payload_encoding: frame.payload_encoding.clone(),
                        payload_type: frame.payload_type.clone(),
                        payload: serde_json::to_vec(&serde_json::json!({ "code": response_code }))?,
                        log_id_new: frame.log_id_new.clone(),
                    },
                )
                .await?;

                result?;
                Ok(None)
            }
            method => Err(anyhow!(
                "Unsupported Feishu websocket frame method: {method}"
            )),
        }
    }

    async fn handle_event_payload(&self, payload: Value) -> Result<()> {
        let Some(message_event) = extract_message_event(&payload) else {
            return Ok(());
        };

        self.dispatcher
            .dispatch_inbound_chat_message(
                &message_event.event_id,
                self.bot_id,
                Some(&message_event.chat_id),
                None,
                Some(&message_event.message_id),
                &message_event.text,
            )
            .await?;
        tracing::info!(
            bot_id = %self.bot_id,
            chat_id = %message_event.chat_id,
            event_id = %message_event.event_id,
            "Feishu inbound text message processed"
        );

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ConnectInfo {
    url: String,
    service_id: i32,
    ping_interval: Duration,
}

#[derive(Debug, Deserialize)]
struct ConnectConfigResponse {
    code: i32,
    msg: String,
    #[serde(rename = "data")]
    data: Option<ConnectConfigData>,
}

#[derive(Debug, Deserialize)]
struct ConnectConfigData {
    #[serde(rename = "URL")]
    url: String,
    #[serde(rename = "ClientConfig")]
    client_config: ConnectClientConfig,
}

#[derive(Debug, Deserialize)]
struct ConnectClientConfig {
    #[serde(rename = "PingInterval")]
    ping_interval: u64,
}

#[derive(Clone, PartialEq, Message)]
struct ProtoHeader {
    #[prost(string, tag = "1")]
    key: String,
    #[prost(string, tag = "2")]
    value: String,
}

#[derive(Clone, PartialEq, Message)]
struct ProtoFrame {
    #[prost(uint64, tag = "1")]
    seq_id: u64,
    #[prost(uint64, tag = "2")]
    log_id: u64,
    #[prost(int32, tag = "3")]
    service: i32,
    #[prost(int32, tag = "4")]
    method: i32,
    #[prost(message, repeated, tag = "5")]
    headers: Vec<ProtoHeader>,
    #[prost(string, tag = "6")]
    payload_encoding: String,
    #[prost(string, tag = "7")]
    payload_type: String,
    #[prost(bytes = "vec", tag = "8")]
    payload: Vec<u8>,
    #[prost(string, tag = "9")]
    log_id_new: String,
}

#[derive(Debug)]
struct CachedChunks {
    created_at: Instant,
    parts: Vec<Option<Vec<u8>>>,
}

#[derive(Debug, Default)]
struct MessageChunkCache {
    chunks: HashMap<String, CachedChunks>,
}

impl MessageChunkCache {
    fn merge(&mut self, frame: &ProtoFrame) -> Result<Option<Value>> {
        self.chunks
            .retain(|_, cached| cached.created_at.elapsed() <= CHUNK_CACHE_TTL);

        let Some(message_id) = frame_header_value(frame, HEADER_MESSAGE_ID) else {
            return Ok(Some(serde_json::from_slice(&frame.payload)?));
        };
        let sum = frame_header_value(frame, HEADER_SUM)
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1);
        let seq = frame_header_value(frame, HEADER_SEQ)
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        if sum <= 1 {
            return Ok(Some(serde_json::from_slice(&frame.payload)?));
        }

        let cached = self
            .chunks
            .entry(message_id.to_string())
            .or_insert_with(|| CachedChunks {
                created_at: Instant::now(),
                parts: vec![None; sum],
            });
        if cached.parts.len() != sum {
            cached.parts = vec![None; sum];
        }
        if seq < cached.parts.len() {
            cached.parts[seq] = Some(frame.payload.clone());
        }
        if cached.parts.iter().all(Option::is_some) {
            let mut merged = Vec::new();
            for part in cached.parts.iter().flatten() {
                merged.extend_from_slice(part);
            }
            self.chunks.remove(message_id);
            return Ok(Some(serde_json::from_slice(&merged)?));
        }

        Ok(None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IncomingTextMessage {
    event_id: String,
    message_id: String,
    chat_id: String,
    text: String,
}

fn extract_message_event(payload: &Value) -> Option<IncomingTextMessage> {
    let event_type = payload
        .pointer("/header/event_type")
        .and_then(Value::as_str)
        .or_else(|| payload.pointer("/event/type").and_then(Value::as_str))
        .or_else(|| payload.get("event_type").and_then(Value::as_str))?;
    if event_type != "im.message.receive_v1" {
        return None;
    }

    let event = payload.get("event").unwrap_or(payload);
    let sender_type = event
        .get("sender")
        .and_then(|sender| sender.get("sender_type"))
        .and_then(Value::as_str);
    if sender_type.is_some_and(|sender_type| sender_type != "user") {
        return None;
    }
    let message = event.get("message")?;
    let message_type = message.get("message_type")?.as_str()?;
    if message_type != "text" {
        return None;
    }

    let content = message.get("content")?.as_str()?;
    let content_value: Value = serde_json::from_str(content).ok()?;
    let text = content_value.get("text")?.as_str()?.trim().to_string();
    if text.is_empty() {
        return None;
    }

    let event_id = payload
        .pointer("/header/event_id")
        .and_then(Value::as_str)
        .or_else(|| payload.get("event_id").and_then(Value::as_str))
        .or_else(|| message.get("message_id").and_then(Value::as_str))?
        .to_string();
    let message_id = message.get("message_id")?.as_str()?.to_string();
    let chat_id = message.get("chat_id")?.as_str()?.to_string();

    Some(IncomingTextMessage {
        event_id,
        message_id,
        chat_id,
        text,
    })
}

fn handle_control_frame(frame: &ProtoFrame) -> Result<Option<Duration>> {
    let Some(message_type) = frame_header_value(frame, HEADER_TYPE) else {
        return Ok(None);
    };
    if message_type == MESSAGE_TYPE_PING {
        return Ok(None);
    }
    if message_type != MESSAGE_TYPE_PONG || frame.payload.is_empty() {
        return Ok(None);
    }

    let payload: Value =
        serde_json::from_slice(&frame.payload).context("Invalid Feishu pong payload")?;
    let ping_interval = payload
        .get("PingInterval")
        .and_then(Value::as_u64)
        .unwrap_or(120);
    Ok(Some(Duration::from_secs(ping_interval.max(1))))
}

fn frame_header_value<'a>(frame: &'a ProtoFrame, key: &str) -> Option<&'a str> {
    frame
        .headers
        .iter()
        .find(|header| header.key == key)
        .map(|header| header.value.as_str())
}

fn extract_service_id(url: &str) -> Result<i32> {
    let parsed = url::Url::parse(url).context("Invalid Feishu websocket URL")?;
    let service_id = parsed
        .query_pairs()
        .find(|(key, _)| key == "service_id")
        .map(|(_, value)| value)
        .context("Feishu websocket URL missing service_id")?;
    service_id
        .parse::<i32>()
        .context("Invalid Feishu websocket service_id")
}

async fn send_frame<Writer>(writer: &mut Writer, frame: ProtoFrame) -> Result<()>
where
    Writer: futures_util::Sink<WsMessage> + Unpin,
    Writer::Error: std::error::Error + Send + Sync + 'static,
{
    let mut bytes = Vec::new();
    frame
        .encode(&mut bytes)
        .context("Failed to encode Feishu websocket frame")?;
    writer
        .send(WsMessage::Binary(bytes))
        .await
        .context("Failed to send Feishu websocket frame")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        IncomingTextMessage, MessageChunkCache, ProtoFrame, ProtoHeader, extract_message_event,
    };

    #[test]
    fn extracts_text_message_event_from_schema_payload() {
        let payload = json!({
            "schema": "2.0",
            "header": {
                "event_type": "im.message.receive_v1",
                "event_id": "evt_123"
            },
            "event": {
                "message": {
                    "message_id": "om_123",
                    "chat_id": "oc_chat_123",
                    "message_type": "text",
                    "content": "{\"text\":\"/vk status\"}"
                }
            }
        });

        let event = extract_message_event(&payload).expect("message event extracts");

        assert_eq!(
            event,
            IncomingTextMessage {
                event_id: "evt_123".into(),
                message_id: "om_123".into(),
                chat_id: "oc_chat_123".into(),
                text: "/vk status".into(),
            }
        );
    }

    #[test]
    fn ignores_non_user_text_messages() {
        let payload = json!({
            "schema": "2.0",
            "header": {
                "event_type": "im.message.receive_v1",
                "event_id": "evt_bot"
            },
            "event": {
                "sender": {
                    "sender_type": "app"
                },
                "message": {
                    "message_id": "om_bot_123",
                    "chat_id": "oc_chat_123",
                    "message_type": "text",
                    "content": "{\"text\":\"bot self message\"}"
                }
            }
        });

        assert!(extract_message_event(&payload).is_none());
    }

    #[test]
    fn merges_chunked_event_payloads() {
        let mut cache = MessageChunkCache::default();
        let payload =
            br#"{"schema":"2.0","header":{"event_type":"im.message.receive_v1","event_id":"evt_1"},"event":{"message":{"chat_id":"chat_1","message_type":"text","content":"{\"text\":\"/vk status\"}"}}}"#;
        let split_index = payload.len() / 2;
        let first = ProtoFrame {
            seq_id: 1,
            log_id: 1,
            service: 1,
            method: 1,
            headers: vec![
                ProtoHeader {
                    key: "type".into(),
                    value: "event".into(),
                },
                ProtoHeader {
                    key: "message_id".into(),
                    value: "msg_1".into(),
                },
                ProtoHeader {
                    key: "sum".into(),
                    value: "2".into(),
                },
                ProtoHeader {
                    key: "seq".into(),
                    value: "0".into(),
                },
            ],
            payload_encoding: String::new(),
            payload_type: String::new(),
            payload: payload[..split_index].to_vec(),
            log_id_new: String::new(),
        };
        let second = ProtoFrame {
            headers: vec![
                ProtoHeader {
                    key: "type".into(),
                    value: "event".into(),
                },
                ProtoHeader {
                    key: "message_id".into(),
                    value: "msg_1".into(),
                },
                ProtoHeader {
                    key: "sum".into(),
                    value: "2".into(),
                },
                ProtoHeader {
                    key: "seq".into(),
                    value: "1".into(),
                },
            ],
            payload: payload[split_index..].to_vec(),
            ..first.clone()
        };

        assert!(cache.merge(&first).expect("first merge").is_none());
        let merged = cache
            .merge(&second)
            .expect("second merge")
            .expect("merged payload");

        assert_eq!(merged["header"]["event_id"], "evt_1");
        assert_eq!(merged["event"]["message"]["chat_id"], "chat_1");
    }
}
