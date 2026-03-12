# sbe-bookticker-app

最小化的 Binance Spot **SBE** `BestBidAskStreamEvent`（bookticker / BBO）接入示例：稳定连接 WS、处理 ping/pong、解码 Binary(SBE) 消息，并把转换后的结果写入日志文件（JSON Lines）。

相关文档：
- [SBE 市场数据流](https://developers.binance.com/docs/zh-CN/binance-spot-api-docs/sbe-market-data-streams)
- [实时订阅/取消数据流（SUBSCRIBE）](https://developers.binance.com/docs/zh-CN/binance-spot-api-docs/web-socket-streams#%E5%AE%9E%E6%97%B6%E8%AE%A2%E9%98%85/%E5%8F%96%E6%B6%88%E6%95%B0%E6%8D%AE%E6%B5%81)

## 运行

### 1) 设置 Ed25519 API Key

Binance 的 SBE 市场数据流要求在 WS 握手时通过 header `X-MBX-APIKEY` 传入 API Key（仅支持 **Ed25519**）。

本程序默认读取环境变量：

- `BINANCE_ED25519_API_KEY`

### 2) 启动

```bash
BINANCE_ED25519_API_KEY=你的key \
cargo run -p sbe-bookticker-app -- --symbol btcusdt --log ./bookticker.log
```

参数说明：

- `--symbol <symbol>`：订阅交易对（会自动转小写），默认 `btcusdt`
- `--log <path>`：日志文件路径（追加写），默认 `./bookticker.log`
- `--ws-url <url>`：WS 地址，默认 `wss://stream-sbe.binance.com:9443/ws`
- `--queue <n>`：日志队列容量（满了会丢弃并按秒汇报），默认 `50000`
- `--api-key <key>`：直接传入 API Key（优先级高于 `BINANCE_ED25519_API_KEY`）

查看帮助：

```bash
cargo run -p sbe-bookticker-app -- --help
```

## 输出格式（JSON Lines）

日志文件每行一条 JSON，字段包括：

- `recv_ts_ms`：本地接收时间（毫秒）
- `event_time_us`：事件时间（微秒）
- `book_update_id`
- `symbol`
- `bid_price/bid_qty/ask_price/ask_qty`：按 exponent 转换成十进制字符串（同时也会输出 mantissa + exponent 便于校验）

示例（字段顺序可能略有差异）：

```json
{"recv_ts_ms":1710000000000,"event_time_us":1710000000123456,"book_update_id":123,"symbol":"btcusdt","bid_price":"65000.12","bid_qty":"0.001","ask_price":"65000.13","ask_qty":"0.002","price_exponent":-2,"qty_exponent":-8,"bid_price_mantissa":6500012,"bid_qty_mantissa":100000,"ask_price_mantissa":6500013,"ask_qty_mantissa":200000}
```

## 关键行为

- **订阅响应**：Text(JSON) 帧；**行情事件**：Binary(SBE) 帧（见 Binance 文档）。
- **Ping/Pong**：服务端大约每 20 秒发 Ping，客户端必须尽快回 Pong，且 payload 需和 Ping 一致。
- **重连**：断线或读写错误后指数退避重连（上限 30s）。

