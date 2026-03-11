# Binance Spot SBE 三套 Schema 详解

> 笔记日期：2026-03-10
>
> 核心结论：Binance Spot 的 SBE 编码分为 **三套独立的 schema**，各自有不同的 XML 定义、schema ID、连接端点和用途。
> 本项目 `spot_sbe` crate 仅覆盖其中一套（REST / WebSocket API），无法解码市场数据流。

---

## 一、三套 Schema 总览

| 维度 | REST / WebSocket API | FIX API | SBE 市场数据流 |
|------|---------------------|---------|---------------|
| **Schema ID** | `3` | `1` | `1` |
| **当前版本** | `3:2`（3:3 滚动部署中） | `1:1` | `1:0` |
| **Schema XML** | [`spot_3_2.xml`](https://github.com/binance/binance-spot-api-docs/blob/master/sbe/schemas/spot_3_2.xml) | [`spot-fixsbe-1_0.xml`](https://github.com/binance/binance-spot-api-docs/blob/master/sbe/schemas/spot-fixsbe-1_0.xml) | [`stream_1_0.xml`](https://github.com/binance/binance-spot-api-docs/blob/master/sbe/schemas/stream_1_0.xml) |
| **Latest 别名** | `spot_prod_latest.xml` | `spot_fix_prod_latest.xml` | — |
| **Rust 包名** | `spot_sbe`（package="spot_sbe"） | — | `spot_stream`（package="spot_stream"） |
| **连接地址** | `api.binance.com`（REST）<br>`ws-api.binance.com`（WS API） | FIX 专用端口 | `stream-sbe.binance.com` |
| **认证方式** | API Key + 签名 | FIX Logon 消息 | Ed25519 API Key（`X-MBX-APIKEY` header） |
| **消息数量** | 100+ 种消息类型 | 20+ 种消息类型 | **仅 4 种**消息类型 |

---

## 二、各套 Schema 详细说明

### 2.1 REST / WebSocket API Schema（本项目 `spot_sbe` 使用的）

**Schema 文件**：`spot_3_2.xml`（schema ID=3, version=2）

**用途**：覆盖 Binance Spot 的所有 REST API 和 WebSocket API **请求-响应**类接口的 SBE 编码。

**典型场景**：

- **查询类**：`exchangeInfo`（交易所信息）、`depth`（深度）、`klines`（K线）、`ticker`（行情快照）等
- **交易类**：`order`（下单/查询/取消）、`orderList`（OCO/OTO/OTOCO）、`sor.order`（SOR 下单）等
- **账户类**：`account`（账户信息）、`myTrades`（成交历史）、`myFilters`（账户过滤器）等
- **用户数据流事件**（通过 WS API 订阅）：`ExecutionReportEvent`、`OutboundAccountPositionEvent`、`ListStatusEvent` 等

**使用方式**：

```bash
# REST API — 通过 Accept 和 X-MBX-SBE header 请求 SBE 格式
curl -X GET \
  -H 'Accept: application/sbe' \
  -H 'X-MBX-SBE: 3:2' \
  'https://api.binance.com/api/v3/exchangeInfo'

# WebSocket API — 通过 URL 参数指定 SBE
wscat -c 'wss://ws-api.binance.com:443/ws-api/v3?responseFormat=sbe&sbeSchemaId=3&sbeSchemaVersion=2'
```

**消息流向**：请求-响应模式。客户端发送请求，服务器返回一个 SBE 编码的响应。

**本项目状态**：`spot_sbe` crate 就是从此 schema 生成的，`sbe-sample-app` 演示了解码 `exchangeInfo` 响应。

---

### 2.2 FIX API Schema

**Schema 文件**：`spot-fixsbe-1_0.xml`（schema ID=1, version=1）

**用途**：为机构级 FIX 协议交易提供 SBE 编码。

**典型场景**：

- **FIX Order Entry**：下单（`NewOrderSingle`）、取消（`OrderCancelRequest`）、执行报告（`ExecutionReport`）等
- **FIX Market Data**：市场数据快照（`MarketDataSnapshot`）、增量刷新（`MarketIncrementalRefresh`）、交易流（`TradeStream`）
- **FIX Drop Copy**：交易回报的旁路副本

**使用方式**：通过 FIX 协议的专用 TCP 连接，在 FIX 消息的 header 中指定使用 SBE 编码。

**特点**：

- 面向专业/机构交易者
- 需要 FIX 会话管理（Logon/Logout/Heartbeat）
- 与 REST/WS API 的 schema 完全独立
- 消息字段使用 FIX 标准 tag 编号

**本项目状态**：不涉及，未生成对应 codec。

---

### 2.3 SBE 市场数据流 Schema ⭐

**Schema 文件**：[`stream_1_0.xml`](https://github.com/binance/binance-spot-api-docs/blob/master/sbe/schemas/stream_1_0.xml)（schema ID=1, version=0, package=`spot_stream`）

**用途**：提供超低延迟的实时市场行情推送，是 JSON WebSocket Streams 的 SBE 替代版本，payload 更小、延迟更低。

**仅包含 4 种消息**：

| 消息名称 | Template ID | Stream 名称 | 更新速度 | 对应的 JSON Stream |
|---------|-------------|-------------|---------|-------------------|
| `TradesStreamEvent` | 10000 | `<symbol>@trade` | 实时 | `<symbol>@trade` |
| `BestBidAskStreamEvent` | 10001 | `<symbol>@bestBidAsk` | 实时 | `<symbol>@bookTicker` |
| `DepthSnapshotStreamEvent` | 10002 | `<symbol>@depth20` | 50ms | `<symbol>@depth20` |
| `DepthDiffStreamEvent` | 10003 | `<symbol>@depth` | 50ms | `<symbol>@depth` |

**连接方式**：

```bash
# 订阅单个 stream
wss://stream-sbe.binance.com/ws/<symbol>@bestBidAsk

# 订阅多个 stream
wss://stream-sbe.binance.com/stream?streams=btcusdt@bestBidAsk/ethusdt@bestBidAsk
```

**认证**：

- 必须在连接时通过 `X-MBX-APIKEY` header 提供 API Key
- **仅支持 Ed25519 密钥**
- 不需要签名（timestamp/signature）
- 不需要额外的 API 权限

**数据接收**：

- 订阅请求以 **JSON text frame** 发送，订阅响应也是 JSON text frame
- 行情数据以 **SBE binary frame** 推送
- 通过 WebSocket frame 类型区分：text = 控制消息，binary = SBE 行情数据

**与 JSON WebSocket Streams 的区别**：

| 维度 | JSON Streams | SBE Market Data Streams |
|------|-------------|------------------------|
| 地址 | `stream.binance.com` | `stream-sbe.binance.com` |
| 编码 | JSON text | SBE binary |
| 认证 | 无需 API Key | 需要 Ed25519 API Key |
| Payload 大小 | 较大 | 更小（约 50%+压缩） |
| 延迟 | 正常 | 更低 |
| 时间戳精度 | 毫秒（默认） | **微秒** |
| 自动剔除 | bookTicker 无 | bestBidAsk 支持 auto-culling |
| 可用 stream 数量 | 20+ 种 | 仅 4 种 |

**本项目状态**：不涉及，需要从 `stream_1_0.xml` 另外生成 codec。

---

## 三、`stream_1_0.xml` Schema 结构分析

`stream_1_0.xml` 的结构非常简洁，适合手写 codec 或用 SbeTool 生成。

### 3.1 基础类型定义

```xml
<!-- 消息头：与 REST/WS API 相同的标准 SBE header -->
<composite name="messageHeader">
    <type name="blockLength" primitiveType="uint16"/>
    <type name="templateId"  primitiveType="uint16"/>
    <type name="schemaId"    primitiveType="uint16"/>
    <type name="version"     primitiveType="uint16"/>
</composite>

<!-- 十进制数：mantissa(int64) + exponent(int8) -->
<type name="mantissa64" primitiveType="int64"/>
<type name="exponent8"  primitiveType="int8"/>

<!-- 时间戳：微秒级 UTC -->
<type name="utcTimestampUs" primitiveType="int64"/>
```

### 3.2 `BestBidAskStreamEvent`（Template ID = 10001）

最优挂单信息，等价于 JSON 的 `bookTicker` 但额外支持 auto-culling 和 `eventTime`。

```xml
<sbe:message name="BestBidAskStreamEvent" id="10001">
    <field id="1" name="eventTime"     type="utcTimestampUs"/>   <!-- 事件时间（微秒） -->
    <field id="2" name="bookUpdateId"  type="updateId"/>          <!-- 订单簿更新 ID -->
    <field id="3" name="priceExponent" type="exponent8"/>         <!-- 价格指数 -->
    <field id="4" name="qtyExponent"   type="exponent8"/>         <!-- 数量指数 -->
    <field id="5" name="bidPrice"      type="mantissa64"/>        <!-- 最优买价 mantissa -->
    <field id="6" name="bidQty"        type="mantissa64"/>        <!-- 最优买量 mantissa -->
    <field id="7" name="askPrice"      type="mantissa64"/>        <!-- 最优卖价 mantissa -->
    <field id="8" name="askQty"        type="mantissa64"/>        <!-- 最优卖量 mantissa -->
    <data id="200" name="symbol"       type="varString8"/>        <!-- 交易对名称 -->
</sbe:message>
```

**内存布局**（不含 8 字节 messageHeader）：

| 偏移 | 字段 | 类型 | 大小 |
|------|------|------|------|
| 0 | eventTime | int64 | 8 |
| 8 | bookUpdateId | int64 | 8 |
| 16 | priceExponent | int8 | 1 |
| 17 | qtyExponent | int8 | 1 |
| 18 | bidPrice | int64 | 8 |
| 26 | bidQty | int64 | 8 |
| 34 | askPrice | int64 | 8 |
| 42 | askQty | int64 | 8 |
| 50 | symbol (length: u8 + UTF-8 data) | varString8 | 1 + N |

**价格/数量还原**：`实际值 = mantissa * 10^exponent`

例如：`bidPrice = 7234560, priceExponent = -2` → 实际买价 = `72345.60`

### 3.3 其他三种消息

**`TradesStreamEvent`（ID=10000）**：逐笔成交

- 固定字段：`eventTime`、`transactTime`、`priceExponent`、`qtyExponent`
- 重复组 `trades`：`id`、`price`、`qty`、`isBuyerMaker`
- 变长数据：`symbol`

**`DepthSnapshotStreamEvent`（ID=10002）**：前 20 档深度快照

- 固定字段：`eventTime`、`bookUpdateId`、`priceExponent`、`qtyExponent`
- 重复组 `bids`：`price`、`qty`
- 重复组 `asks`：`price`、`qty`
- 变长数据：`symbol`

**`DepthDiffStreamEvent`（ID=10003）**：增量深度更新

- 固定字段：`eventTime`、`firstBookUpdateId`、`lastBookUpdateId`、`priceExponent`、`qtyExponent`
- 重复组 `bids`：`price`、`qty`
- 重复组 `asks`：`price`、`qty`
- 变长数据：`symbol`

---

## 四、如何生成市场数据流的 Rust Decoder

### 方案 A：用 SbeTool 从 `stream_1_0.xml` 自动生成

```bash
# 1. 下载 schema
curl -o stream_1_0.xml \
  https://raw.githubusercontent.com/binance/binance-spot-api-docs/master/sbe/schemas/stream_1_0.xml

# 2. 克隆并构建 SbeTool（与生成 spot_sbe 时相同）
git clone https://github.com/real-logic/simple-binary-encoding.git --branch 1.35.6
cd simple-binary-encoding
./gradlew

# 3. 生成 Rust codec（输出到 spot_stream 目录）
java \
  -Dsbe.output.dir=. \
  -Dsbe.target.language=Rust \
  -jar sbe-all/build/libs/sbe-all-1.35.6.jar \
  ../stream_1_0.xml

# 4. 整理生成的代码
cd spot_stream
cargo fmt
cargo clippy --fix --allow-dirty
```

生成后会得到一个 `spot_stream` crate，包含：
- `best_bid_ask_stream_event_codec.rs`
- `trades_stream_event_codec.rs`
- `depth_snapshot_stream_event_codec.rs`
- `depth_diff_stream_event_codec.rs`
- `message_header_codec.rs`
- 以及相关类型定义

使用方式与 `spot_sbe` 完全一致：`ReadBuf::new(&data)` → `MessageHeaderDecoder` → 按 `templateId` 分发到对应 Decoder。

### 方案 B：手写 Codec（参考 Nautilus Trader）

由于只有 4 种消息且结构简单，可以直接基于 `ReadBuf` 手写零拷贝 decoder。

[Nautilus Trader](https://nautilustrader.io/docs/core-latest/nautilus_binance/common/sbe/index.html) 的 `nautilus_binance::common::sbe` 模块就是这样做的：

- `sbe::stream` — 手写的 4 种市场数据流 codec（schema 1:0）
- `sbe::spot` — SbeTool 生成的 REST/WS API codec（schema 3:2）

手写的好处是可以精确控制内存布局和错误处理，但需要自己维护与 schema 的一致性。

### 推荐

对于本项目，**方案 A（SbeTool 生成）更合适**：
- 与现有 `spot_sbe` 的生成方式一致
- 如果 Binance 后续更新 `stream_1_0.xml`（如添加新字段或新消息），重新生成即可
- 生成的 codec 与 `spot_sbe` 共享相同的 `ReadBuf` / `WriteBuf` 基础设施

---

## 五、项目结构建议（如后续扩展）

```
binance-sbe-rust-sample-app/
├── spot_sbe/                  # schema 3:2 — REST/WS API codec（已有）
│   └── src/lib.rs             # SBE_SCHEMA_ID=3, SBE_SCHEMA_VERSION=2
├── spot_stream/               # schema 1:0 — 市场数据流 codec（待生成）
│   └── src/lib.rs             # SBE_SCHEMA_ID=1, SBE_SCHEMA_VERSION=0
├── sbe-sample-app/            # exchangeInfo 解码示例（已有）
├── stream-sample-app/         # 市场数据流订阅 + 解码示例（待开发）
│   └── src/main.rs
└── tools/
    └── websocket_send.py      # WebSocket API 辅助工具（已有）
```

---

## 六、参考链接

- [SBE FAQ（生成 decoder 的完整指引）](https://developers.binance.com/docs/binance-spot-api-docs/faqs/sbe_faq)
- [SBE 市场数据流文档](https://developers.binance.com/docs/binance-spot-api-docs/sbe-market-data-streams)
- [REST/WS API Schema (spot_3_2.xml)](https://github.com/binance/binance-spot-api-docs/blob/master/sbe/schemas/spot_3_2.xml)
- [市场数据流 Schema (stream_1_0.xml)](https://github.com/binance/binance-spot-api-docs/blob/master/sbe/schemas/stream_1_0.xml)
- [FIX SBE Schema (spot-fixsbe-1_0.xml)](https://github.com/binance/binance-spot-api-docs/blob/master/sbe/schemas/spot-fixsbe-1_0.xml)
- [Schema 生命周期（版本发布/废弃/退役时间线）](https://github.com/binance/binance-spot-api-docs/blob/master/sbe/schemas/sbe_schema_lifecycle_prod.json)
- [Binance Changelog（SBE 相关更新记录）](https://developers.binance.com/docs/binance-spot-api-docs)
- [Nautilus Trader SBE 实现参考](https://nautilustrader.io/docs/core-latest/nautilus_binance/common/sbe/index.html)
- [Binance 官方 Rust Sample App](https://github.com/binance/binance-sbe-rust-sample-app)
