# BookTicker Benchmark: SBE vs JSON

Binance 现货 BookTicker（最优挂单信息）延迟对比工具。同时连接 SBE 二进制流和 JSON 文本流，按相同的 `update_id` 匹配，测量本地接收时间差，统计 SBE 比 JSON 快多少。

## 原理

1. 同时建立两条 WebSocket 连接：
   - **SBE**: `wss://stream-sbe.binance.com:9443/ws`，订阅 `<symbol>@bestBidAsk`（需要 Ed25519 API Key）
   - **JSON**: `wss://stream.binance.com:9443/ws/<symbol>@bookTicker`（无需认证）
2. 收到每帧数据时，立即记录 `Instant::now()`（**解析之前**），然后解析提取 `update_id`
3. 用 `HashMap` 按 `update_id` 配对，计算时间差：`diff = json_recv - sbe_recv`
   - **正数** = SBE 先到达（SBE 更快）
   - **负数** = JSON 先到达（SBE 更慢）
4. 到达设定时长后自动停止，输出百分位统计报告

## 前置条件

- Rust 1.73+
- Binance Ed25519 API Key（仅用于 SBE 流认证，无需额外权限）

## 快速开始

```bash
# 设置环境变量
export BINANCE_ED25519_API_KEY="your-ed25519-api-key"

# 默认：BTCUSDT，60 秒
cargo run -p bookticker-benchmark

# 自定义币种和时长
cargo run -p bookticker-benchmark -- --symbol ethusdt --duration 300

# Release 模式（推荐，减少解析开销）
cargo run -p bookticker-benchmark --release -- --symbol btcusdt --duration 120
```

## 命令行参数

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `--symbol <symbol>` | 交易对（小写） | `btcusdt` |
| `--duration <seconds>` | 采集时长（秒） | `60` |
| `--api-key <key>` | Ed25519 API Key（优先级高于环境变量） | - |
| `--sbe-url <url>` | SBE WebSocket 地址 | `wss://stream-sbe.binance.com:9443/ws` |
| `--json-url <url>` | JSON WebSocket 地址 | `wss://stream.binance.com:9443` |
| `-h, --help` | 显示帮助 | - |

## 输出示例

```
========== Binance BookTicker: SBE vs JSON Latency Benchmark ==========
Symbol:            BTCUSDT
Duration:          60s
SBE events:        12345
JSON events:       12340
Matched pairs:     12300
Unmatched (SBE):   45
Unmatched (JSON):  40

Latency (positive = SBE faster):
  Min:   -0.532 ms
  P25:   +0.120 ms
  P50:   +0.350 ms
  P75:   +0.680 ms
  P90:   +1.120 ms
  P95:   +1.560 ms
  P99:   +2.890 ms
  Max:   +5.230 ms
  Mean:  +0.420 ms
=========================================================================
```

## 注意事项

- 延迟数据受网络环境影响，建议在与 Binance 服务器网络延迟较低的环境中测试
- SBE 流使用 auto-culling 机制，高负载时可能丢弃过时事件，导致部分 update_id 无法匹配
- 推荐使用 `--release` 编译以最小化解析开销对结果的影响
