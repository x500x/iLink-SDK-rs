# 命令行工具

`cli` feature（默认开启）会构建一个小的 `weixin` 二进制，它包装了本库，这样你可以在
写代码之前端到端地验证协议。CLI 读取与库相同的状态目录与 `openclaw.json`。

## 构建与运行

```bash
cargo run -- --help          # 显示所有命令
cargo run -- login           # 二维码登录，保存凭证
cargo run -- accounts        # 列出已注册账户
cargo run -- status          # 解析后的账户设置
cargo run -- send <@im.wechat> "hi" --context-token <token>
cargo run -- send-file <@im.wechat> ./photo.png --caption "看这张"
cargo run -- listen          # 长轮询并打印入站消息
cargo run -- logout <account-id>
```

> 使用 `--no-default-features` 时不会构建该二进制。

## 命令

| 命令 | 描述 |
| --- | --- |
| `login` | 运行二维码登录流程并持久化得到的 bot token。渲染待扫描的二维码。 |
| `accounts` | 列出所有已注册账户 id（来自 `accounts.json`）。 |
| `status` | 打印活动账户的解析设置（base URL、CDN URL、是否启用、route tag、配置来源）。 |
| `send <to> <text> [--context-token <t>]` | 向 `to` 发送文本消息。 |
| `send-file <to> <path> [--caption <c>]` | 把 `path` 上传到 CDN 并发送（按扩展名判定图片/视频/文件）。 |
| `listen` | 长轮询 `getUpdates` 并在消息到达时打印。 |
| `logout <account-id>` | 从状态目录移除一个账户及其文件。 |

## 状态与配置

CLI 使用与库相同的解析规则：

- 状态目录：`OPENCLAW_STATE_DIR` → `CLAWDBOT_STATE_DIR` →
  `$XDG_STATE_HOME/openclaw` → `$HOME/.openclaw`。
- 配置：`OPENCLAW_CONFIG` → `<state_dir>/openclaw.json`，区块
  `channels.openclaw-weixin`。

因此 CLI 执行的二维码登录可立即被你自己的 Rust 代码使用（反之亦然），同一台机器上的
TypeScript 客户端也会共享这些凭证。

## 日志

CLI 会初始化一个 `tracing-subscriber`（经由 `logging` 模块），由 `RUST_LOG` 驱动
（例如 `RUST_LOG=ilink_sdk=debug cargo run -- listen`）。
