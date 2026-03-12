use anyhow::{anyhow, Context as _, Result};
use futures_util::{SinkExt as _, StreamExt as _};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt as _;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::tungstenite::protocol::Message;

mod nautilus_sbe;

use crate::nautilus_sbe::stream::BestBidAskStreamEvent;

#[derive(Debug, Clone)]
struct Config {
    ws_url: String,
    api_key: String,
    symbol: String,
    log_path: PathBuf,
    queue_capacity: usize,
}

impl Config {
    fn from_env_and_args() -> Result<Self> {
        let mut ws_url = "wss://stream-sbe.binance.com:9443/ws".to_string();
        let mut symbol = "btcusdt".to_string();
        let mut log_path = PathBuf::from("bookticker.log");
        let mut queue_capacity: usize = 50_000;
        let mut api_key: Option<String> = std::env::var("BINANCE_ED25519_API_KEY").ok();

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--ws-url" => {
                    ws_url = args.next().ok_or_else(|| anyhow!("--ws-url 需要一个值"))?;
                }
                "--symbol" => {
                    symbol = args.next().ok_or_else(|| anyhow!("--symbol 需要一个值"))?;
                }
                "--log" => {
                    log_path =
                        PathBuf::from(args.next().ok_or_else(|| anyhow!("--log 需要一个值"))?);
                }
                "--queue" => {
                    let raw = args.next().ok_or_else(|| anyhow!("--queue 需要一个值"))?;
                    queue_capacity = raw.parse::<usize>().context("--queue 需要是数字")?;
                }
                "--api-key" => {
                    api_key = Some(args.next().ok_or_else(|| anyhow!("--api-key 需要一个值"))?);
                }
                "-h" | "--help" => {
                    print_help_and_exit();
                }
                other => {
                    return Err(anyhow!(
                        "未知参数: {other}\n\n运行 `sbe-bookticker-app --help` 查看用法"
                    ));
                }
            }
        }

        if symbol.is_empty() {
            return Err(anyhow!("symbol 不能为空"));
        }
        let symbol = symbol.to_ascii_lowercase();

        let api_key = api_key.ok_or_else(|| {
            anyhow!("缺少 Ed25519 API Key：请设置环境变量 BINANCE_ED25519_API_KEY 或传 --api-key")
        })?;

        Ok(Self {
            ws_url,
            api_key,
            symbol,
            log_path,
            queue_capacity,
        })
    }
}

fn print_help_and_exit() -> ! {
    eprintln!(
        r#"用法:
  BINANCE_ED25519_API_KEY=... cargo run -p sbe-bookticker-app -- --symbol btcusdt --log ./bookticker.log

参数:
  --symbol <symbol>     订阅交易对（小写），默认 btcusdt
  --log <path>          日志文件路径（追加写），默认 ./bookticker.log
  --ws-url <url>        WS 地址，默认 wss://stream-sbe.binance.com:9443/ws
  --queue <n>           日志队列容量（满了会丢弃），默认 50000
  --api-key <key>       API Key（优先级高于环境变量 BINANCE_ED25519_API_KEY）
  -h, --help            显示帮助

说明:
  - 订阅响应是 Text(JSON) 帧；行情事件是 Binary(SBE) 帧
  - 服务端每 20 秒左右发 Ping，本程序会立刻回 Pong（payload 一致）"#
    );
    std::process::exit(0);
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env_and_args()?;

    let (stop_tx, stop_rx) = watch::channel(false);
    let (log_tx, log_rx) = mpsc::channel::<String>(cfg.queue_capacity);

    let writer_handle = tokio::spawn(writer_task(cfg.log_path.clone(), log_rx));
    let ws_handle = tokio::spawn(ws_reconnect_loop(
        cfg.clone(),
        log_tx.clone(),
        stop_rx.clone(),
    ));

    tokio::signal::ctrl_c().await?;
    let _ = stop_tx.send(true);
    drop(log_tx);

    if let Err(err) = ws_handle.await.context("ws task join")? {
        eprintln!("ws 任务退出: {err:?}");
    }
    if let Err(err) = writer_handle.await.context("writer task join")? {
        eprintln!("writer 任务退出: {err:?}");
    }

    Ok(())
}

async fn writer_task(log_path: PathBuf, mut rx: mpsc::Receiver<String>) -> Result<()> {
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .await
        .with_context(|| format!("打开日志文件失败: {}", log_path.display()))?;

    let mut writer = tokio::io::BufWriter::new(file);
    let mut flush_interval = tokio::time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            maybe_line = rx.recv() => {
                match maybe_line {
                    Some(line) => {
                        writer.write_all(line.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                    }
                    None => break,
                }
            }
            _ = flush_interval.tick() => {
                writer.flush().await?;
            }
        }
    }

    writer.flush().await?;
    Ok(())
}

async fn ws_reconnect_loop(
    cfg: Config,
    log_tx: mpsc::Sender<String>,
    mut stop_rx: watch::Receiver<bool>,
) -> Result<()> {
    let mut backoff = Duration::from_millis(250);
    let max_backoff = Duration::from_secs(30);

    loop {
        if *stop_rx.borrow() {
            return Ok(());
        }

        let res = run_ws_session(&cfg, &log_tx, &mut stop_rx).await;
        match res {
            Ok(()) => {
                backoff = Duration::from_millis(250);
            }
            Err(err) => {
                eprintln!("ws session 错误: {err:?}，{backoff:?} 后重连");
                tokio::select! {
                    _ = stop_rx.changed() => return Ok(()),
                    _ = tokio::time::sleep(backoff) => {}
                }
                backoff = std::cmp::min(max_backoff, backoff.saturating_mul(2));
            }
        }
    }
}

async fn run_ws_session(
    cfg: &Config,
    log_tx: &mpsc::Sender<String>,
    stop_rx: &mut watch::Receiver<bool>,
) -> Result<()> {
    let mut req = cfg
        .ws_url
        .as_str()
        .into_client_request()
        .context("构造 WS request 失败")?;
    req.headers_mut().insert(
        "X-MBX-APIKEY",
        cfg.api_key
            .parse()
            .context("API key 不是合法 header value")?,
    );

    let (mut ws, _resp) = connect_async(req).await.context("WS 连接失败")?;

    let stream = format!("{}@bestBidAsk", cfg.symbol);
    let subscribe = format!(r#"{{"method":"SUBSCRIBE","params":["{stream}"],"id":1}}"#);
    ws.send(Message::Text(subscribe))
        .await
        .context("发送 SUBSCRIBE 失败")?;

    let mut dropped: u64 = 0;
    let mut last_drop_report = tokio::time::Instant::now();

    loop {
        tokio::select! {
            _ = stop_rx.changed() => {
                let _ = ws.close(None).await;
                return Ok(());
            }
            maybe_msg = ws.next() => {
                let msg = match maybe_msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return Err(anyhow!("WS read 错误: {e}")),
                    None => return Err(anyhow!("WS 连接关闭")),
                };

                match msg {
                    Message::Ping(payload) => {
                        // println!("ws ping: {:?}", &payload);
                        ws.send(Message::Pong(payload)).await.context("回 PONG 失败")?;
                    }
                    Message::Pong(_) => {}
                    Message::Text(txt) => {
                        // 订阅响应/错误都走 text 帧，直接打出来便于排查。
                        eprintln!("ws text: {txt}");
                    }
                    Message::Binary(bin) => {
                        match BestBidAskStreamEvent::decode(&bin) {
                            Ok(ev) => {
                                let line = format_event_as_json_line(&ev);
                                if log_tx.try_send(line).is_err() {
                                    dropped += 1;
                                    if last_drop_report.elapsed() >= Duration::from_secs(1) {
                                        eprintln!("日志队列已满，累计丢弃 {dropped} 条");
                                        last_drop_report = tokio::time::Instant::now();
                                    }
                                }
                            }
                            Err(err) => {
                                eprintln!("解码 BestBidAskStreamEvent 失败: {err}");
                            }
                        }
                    }
                    Message::Close(frame) => {
                        return Err(anyhow!("WS close: {frame:?}"));
                    }
                    _ => {}
                }
            }
        }
    }
}

fn format_event_as_json_line(ev: &BestBidAskStreamEvent) -> String {
    let recv_ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    let bid_price = mantissa_to_decimal_string(ev.bid_price_mantissa, ev.price_exponent);
    let bid_qty = mantissa_to_decimal_string(ev.bid_qty_mantissa, ev.qty_exponent);
    let ask_price = mantissa_to_decimal_string(ev.ask_price_mantissa, ev.price_exponent);
    let ask_qty = mantissa_to_decimal_string(ev.ask_qty_mantissa, ev.qty_exponent);

    let symbol = json_escape_string(&ev.symbol);

    format!(
        r#"{{"recv_ts_ms":{recv_ts_ms},"event_time_us":{event_time_us},"book_update_id":{book_update_id},"symbol":"{symbol}","bid_price":"{bid_price}","bid_qty":"{bid_qty}","ask_price":"{ask_price}","ask_qty":"{ask_qty}","price_exponent":{price_exponent},"qty_exponent":{qty_exponent},"bid_price_mantissa":{bid_price_mantissa},"bid_qty_mantissa":{bid_qty_mantissa},"ask_price_mantissa":{ask_price_mantissa},"ask_qty_mantissa":{ask_qty_mantissa}}}"#,
        event_time_us = ev.event_time_us,
        book_update_id = ev.book_update_id,
        price_exponent = ev.price_exponent,
        qty_exponent = ev.qty_exponent,
        bid_price_mantissa = ev.bid_price_mantissa,
        bid_qty_mantissa = ev.bid_qty_mantissa,
        ask_price_mantissa = ev.ask_price_mantissa,
        ask_qty_mantissa = ev.ask_qty_mantissa,
    )
}

fn json_escape_string(s: &str) -> String {
    // 足够应对交易对这种字段；只做最基本的 JSON 转义，避免破坏日志格式。
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn mantissa_to_decimal_string(mantissa: i64, exponent: i8) -> String {
    if exponent == 0 {
        return mantissa.to_string();
    }

    let sign = if mantissa < 0 { "-" } else { "" };
    let mut digits = mantissa.unsigned_abs().to_string();

    if exponent > 0 {
        digits.push_str(&"0".repeat(exponent as usize));
        return format!("{sign}{digits}");
    }

    let exp = (-exponent) as usize;
    if digits.len() > exp {
        let split = digits.len() - exp;
        let (int_part, frac_part) = digits.split_at(split);
        let frac_trimmed = frac_part.trim_end_matches('0');
        if frac_trimmed.is_empty() {
            format!("{sign}{int_part}")
        } else {
            format!("{sign}{int_part}.{frac_trimmed}")
        }
    } else {
        let zeros = "0".repeat(exp - digits.len());
        let frac = format!("{zeros}{digits}");
        let frac_trimmed = frac.trim_end_matches('0');
        if frac_trimmed.is_empty() {
            format!("{sign}0")
        } else {
            format!("{sign}0.{frac_trimmed}")
        }
    }
}
