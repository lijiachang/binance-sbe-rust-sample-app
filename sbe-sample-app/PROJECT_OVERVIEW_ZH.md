# sbe-sample-app 项目详解

## 1. 项目定位

`sbe-sample-app` 是一个 Rust 命令行示例程序，用于将 Binance Spot API 返回的 **SBE（二进制）exchangeInfo 响应** 解码为结构化数据，并输出为 YAML。

它的核心价值是：

- 演示如何使用 `spot_sbe`（由 SBE 工具生成的 Rust 解码器）解析二进制消息
- 演示 REST 和 WebSocket 两类 exchangeInfo 响应的统一解码思路
- 提供一套可读性高的 YAML 输出，便于验证字段和联调

在整个 workspace 中，它依赖同级 crate `spot_sbe`：

- `spot_sbe`: 自动生成的协议编解码代码（大量 message/filter decoder）
- `sbe-sample-app`: 人工编写的业务组装与输出层

---

## 2. 技术栈与依赖

`sbe-sample-app/Cargo.toml` 的依赖非常精简：

- `anyhow`: 错误处理与快速失败（`bail!`）
- `serde` + `serde_yaml`: 序列化为 YAML
- `spot_sbe`（path 依赖）: Binance Spot SBE 协议的 Rust 解码器

工具链固定在根目录 `rust-toolchain.toml`：

- `nightly-2023-08-01`

---

## 3. 目录结构与模块职责

当前 `sbe-sample-app` 的代码规模不大，职责划分清晰：

- `src/main.rs`
  - 程序入口
  - 从 `STDIN` 读取二进制 payload
  - 解析消息头与模板类型
  - 分支处理错误响应 / WebSocket 包装响应 / exchangeInfo 正常响应
  - 最终组装为 Rust 结构并输出 YAML

- `src/exchange_info.rs`
  - 定义 exchangeInfo 领域模型（`ExchangeInfo`, `SymbolInfo`, `ExchangeFilter`, `SymbolFilter`, `Sor` 等）
  - 定义 `Decimal`（mantissa + exponent）结构，保留协议原始小数表达
  - 对多种 SBE enum/bitset 做自定义序列化，输出可读字符串

- `src/rate_limit.rs`
  - 定义 `RateLimit` 结构
  - 将 `RateLimitType` 和 `RateLimitInterval` 从协议枚举转换为字符串输出

- `src/websocket.rs`
  - 定义 WebSocket 元信息结构 `WebSocketMetadata`
  - 统一承载 `status/rateLimits/id/result`
  - `result` 既可以是 `Error`，也可以是 `ExchangeInfo`

---

## 4. 输入输出模型

## 输入

程序通过标准输入接收二进制数据（`read_payload(io::stdin())`）：

- REST 场景：直接是 `exchangeInfo` SBE 响应
- WebSocket 场景：先是 WS 包装层，再嵌套业务 `result`（可能是 error，也可能是 exchangeInfo）

## 输出

程序输出 YAML 到标准输出（`println!`）。

若遇到错误响应，程序会将错误体序列化后以 `bail!(yaml)` 方式返回错误（调用方能看到可读错误内容）。

---

## 5. 主流程（main.rs）逐步说明

可以把 `main` 的逻辑理解为 “识别消息类型 -> 解码对应结构 -> 输出统一 YAML”：

1. 读取输入字节流  
   使用 `read_to_end` 将 STDIN 全量读入内存。

2. 解析 SBE Message Header  
   通过 `MessageHeaderDecoder` 获取 `template_id/schema_id/version`。

3. 优先识别通用错误消息  
   若 `template_id` 是 `error_response`，直接调用 `decode_error` 并输出 YAML 错误。

4. 校验 schema  
   - `schema_id` 必须等于 `exchange_info_response` 的 schema id，否则直接报错  
   - `version` 不一致只警告（同 schema id 预期向后兼容）

5. 处理 WebSocket 包装层（如存在）  
   若首层模板是 `web_socket_response`：
   - 解出 `status/rateLimits/id`
   - 读取 `result` 在 payload 中的偏移
   - 重新从 `result` 切片位置创建新 header decoder
   - 若 `result` 是 error，写入 `WebSocketMetadata.result = Error`

6. 解析 exchangeInfo 主体  
   按组依次读取：
   - `rate_limits`
   - `exchange_filters`
   - `symbols`（每个 symbol 继续解析嵌套 `filters` 与 `permission_sets`）
   - `sors`

7. 组装与输出  
   - REST：直接输出 `ExchangeInfo` YAML  
   - WebSocket：输出带元信息的 `WebSocketMetadata` YAML

---

## 6. 关键解码策略

## 6.1 Template ID 分发

`decode_exchange_filter` 与 `decode_symbol_filter` 都通过 `template_id` 做分发：

- 匹配已知模板 -> 进入对应 decoder，构造 enum 变体
- 未知模板 -> 立即 `bail!`，避免静默吞错

这种写法对协议演进很友好：新增 filter 模板时，编译期和运行期都容易定位修改点。

## 6.2 布尔值安全转换

SBE 中布尔是 `BoolEnum`（`True/False/NullVal`）。  
`into_bool` 对 `NullVal` 直接报错，防止把不合法值默默当作 `false`。

## 6.3 嵌套 decoder 的 parent 回溯

SBE group 解码经常要 “下钻子结构 -> 返回父级继续读”。代码中多处使用 `decoder.parent()?`，例如：

- symbol 下的 `filters_decoder`
- symbol 下的 `permission_sets_decoder` 与其内部 `permissions_decoder`

这是读取复杂嵌套结构时最关键的正确性要点之一。

## 6.4 Decimal 保留原始精度语义

价格/数量等字段并不直接转成 `f64`，而是存成：

- `mantissa: i64`
- `exponent: i8`

优点是避免浮点误差，并保持和协议字段一一对应，便于后续在业务层自行做高精度计算。

---

## 7. 数据结构层设计亮点

- `#[serde(rename_all = "camelCase")]`：输出字段与 Binance 风格一致
- `SymbolFilter/ExchangeFilter` 使用 `#[serde(tag = "filterType")]`：
  - YAML 中保留 filter 类型标签
  - 不同过滤器字段清晰可辨
- 多个协议 enum/bitset 都序列化为字符串列表（如 `ORDER_TYPES`、`STP modes`）：
  - 可读性好
  - 便于人类排查与脚本消费

---

## 8. 典型使用方式

## 8.1 REST exchangeInfo

```bash
cargo build -p sbe-sample-app
curl -X GET -H 'Accept: application/sbe' -H 'X-MBX-SBE: 3:2' \
  'https://api.binance.com/api/v3/exchangeInfo' \
  | ./target/debug/sbe-sample-app
```

## 8.2 WebSocket exchangeInfo

```bash
echo '{"id":"93fb61ef-89f8-4d6e-b022-4f035a3fadad","method":"exchangeInfo","params":{"symbol":"BTCUSDT"}}' \
  | ../tools/websocket_send.py 'wss://ws-api.binance.com:443/ws-api/v3?responseFormat=sbe&sbeSchemaId=3&sbeSchemaVersion=2' \
  | ./target/debug/sbe-sample-app
```

`tools/websocket_send.py` 的作用很简单：从 stdin 读取 JSON 请求，发给 WS，接收二进制响应并原样写到 stdout，方便和本程序管道拼接。

---

## 9. 边界与限制

- 当前样例聚焦 `exchangeInfo`，并未实现全 API 消息的业务封装
- 输入一次性读入内存，适合响应包场景，不是流式处理器
- 对未知模板、非法布尔值采用快速失败策略，安全但较严格
- 输出是 YAML，可读性优先，不追求最小体积

---

## 10. 扩展建议

如果你要把该样例扩展成更完整的解码工具，建议按以下顺序推进：

1. 新增 endpoint 的 message 分发层（按 `template_id` 映射到业务模型）
2. 抽出通用 group 解码工具函数，减少重复样板代码
3. 增加 JSON 输出选项（CLI 参数控制）
4. 为关键 decode 路径补充 golden file 测试（输入二进制 -> 断言 YAML/JSON）
5. 引入 schema/version 能力矩阵，支持多版本兼容行为配置

---

## 11. 一句话总结

`sbe-sample-app` 是一个小而完整的 SBE 解码示例：它把 Binance Spot 的 `exchangeInfo` 二进制消息可靠地还原为可读 YAML，并展示了 Rust 下处理 SBE 协议、嵌套 group、模板分发与序列化映射的标准实践。
