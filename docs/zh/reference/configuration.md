# 配置

SDK 从 `openclaw.json`（与 TypeScript 插件相同的文件）以及若干环境变量读取可选配置。
没有必需的配置——合理的默认值覆盖了常见情况。

## `openclaw.json` 布局

channel 区块为 `channels.openclaw-weixin`。文件由
[`resolve_config_path`](../../../src/storage/config.rs) 解析：

1. `$OPENCLAW_CONFIG`（若设置且非空）
2. `<state_dir>/openclaw.json`

```jsonc
{
  "channels": {
    "openclaw-weixin": {
      "botAgent": "MyBot/1.0.0",
      "routeTag": "r1",
      "replyProgressMessages": false,
      "quoteCache": {
        "enabled": true,
        "retentionDays": 30,
        "maxMessagesPerAccount": 10000,
        "mediaRetentionDays": 7,
        "maxMediaBytesPerAccount": 268435456,
        "maxSingleMediaBytes": 26214400
      },
      "accounts": {
        "deadbeef-im-bot": {
          "baseUrl": "https://ilinkai.weixin.qq.com",
          "cdnBaseUrl": "https://novac2c.cdn.weixin.qq.com/c2c",
          "enabled": true,
          "name": "primary",
          "routeTag": "r2"
        }
      }
    }
  }
}
```

## channel 级键

| 键 | 类型 | 默认 | 用途 |
| --- | --- | --- | --- |
| `botAgent` | string | `OpenClaw` | 作为 `base_info.bot_agent` 发送的可观测性标识。非认证凭据。 |
| `routeTag` | string \| number | — | 作为 `SKRouteTag` 头转发。 |
| `replyProgressMessages` | bool | `true` | 是否发送结构化的工具调用进度消息。 |
| `quoteCache` | object | 见下表 | 本地引用缓存策略（见下文）。 |
| `accounts.<id>` | object | — | 每账户覆盖。 |

### `botAgent` 净化

`botAgent` 接受 UA 风格的 `Name/Version` token，可带可选 `(comment)`。语法：`name` 与
`version` 为字母数字加 `_ . + -`（各最多 32 字符）；comment 为可打印 ASCII 减 `(` `)`
（最多 64 字符）。非法 token 被丢弃；空结果回退到 `OpenClaw`。超过 256 字节
（`BOT_AGENT_MAX_LEN`）的结果通过丢弃后续 token 截断。

```
MyBot/1.0.0
MyBot/1.0.0 (region=cn;env=prod)
MyBot/1.0.0 LangChain/0.3.5
```

## 每账户键（`accounts.<id>`）

| 键 | 类型 | 默认 | 用途 |
| --- | --- | --- | --- |
| `baseUrl` | string | `https://ilinkai.weixin.qq.com` | 每账户 API base URL。 |
| `cdnBaseUrl` | string | `https://novac2c.cdn.weixin.qq.com/c2c` | 每账户 CDN base URL。 |
| `enabled` | boolean | `true` | 禁用账户但不删除它。 |
| `name` | string | — | 友好名称（仅供内省）。 |
| `routeTag` | string \| number | — | 每账户的 `SKRouteTag` 覆盖。 |

账户级 `baseUrl` / `cdnBaseUrl` / `routeTag` 优先于 channel 级，后者优先于内置默认值。

## 引用缓存键（`quoteCache`）

| 键 | 类型 | 默认 |
| --- | --- | --- |
| `enabled` | boolean | `true` |
| `retentionDays` | number | 30 |
| `maxMessagesPerAccount` | number | 10000 |
| `mediaRetentionDays` | number | 7 |
| `maxMediaBytesPerAccount` | number | 256 MiB (268435456) |
| `maxSingleMediaBytes` | number | 25 MiB (26214400) |

非法值回退到默认值。见[存储与状态](../guides/storage.md)。

## 运行时覆盖

其中大部分也可以在代码中直接设置，无需改动磁盘：`WeixinClientBuilder` 暴露了
`base_url`、`cdn_base_url`、`route_tag`、`bot_agent`、`timeout_ms`；`WeixinApiOptions`
暴露了 `token`、`timeout_ms`、`route_tag`、`cdn_base_url` 以及 `with_*` 构造器；并且
[`resolve_channel_config`] 接受一个内存中的 [`SdkConfig`] 来覆盖磁盘上的全部内容。

## 环境变量

| 变量 | 效果 |
| --- | --- |
| `OPENCLAW_STATE_DIR` | 状态目录根（最高优先级）。 |
| `CLAWDBOT_STATE_DIR` | 回退状态目录。 |
| `OPENCLAW_CONFIG` | `openclaw.json` 的路径（覆盖 `<state_dir>/openclaw.json`）。 |
| `XDG_STATE_HOME` | Unix：作为 `$XDG_STATE_HOME/openclaw` 的基准（当前述变量均未设置时使用）。 |
| `HOME` / `USERPROFILE` | 作为 `~/.openclaw` 的基准（最后兜底）。 |

## Feature 开关

| Feature | 默认 | 作用 |
| --- | --- | --- |
| `cli` | 开启 | 构建 `weixin` 二进制（`clap`、`qrcode`、`directories`、`tracing-subscriber`）。 |

```bash
cargo build --no-default-features        # 仅库，无 CLI
cargo build --features cli               # 库 + CLI（默认）
```
