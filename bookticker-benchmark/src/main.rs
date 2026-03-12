use anyhow::{anyhow, Context as _, Result};
use futures_util::{SinkExt as _, StreamExt as _};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::tungstenite::protocol::Message;

mod nautilus_sbe;

use crate::nautilus_sbe::stream::BestBidAskStreamEvent;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Config {
    sbe_url: String,
    json_url: String,
    api_key: String,
    symbol: String,
    duration_secs: u64,
}

impl Config {
    fn from_env_and_args() -> Result<Self> {
        let mut sbe_url = "wss://stream-sbe.binance.com:9443/ws".to_string();
        let mut json_url = "wss://stream.binance.com:9443".to_string();
        let mut symbol = "btcusdt".to_string();
        let mut duration_secs: u64 = 60;
        let mut api_key: Option<String> = std::env::var("BINANCE_ED25519_API_KEY").ok();

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--symbol" => {
                    symbol = args.next().ok_or_else(|| anyhow!("--symbol requires a value"))?;
                }
                "--duration" => {
                    let raw = args
                        .next()
                        .ok_or_else(|| anyhow!("--duration requires a value"))?;
                    duration_secs = raw.parse::<u64>().context("--duration must be a number")?;
                }
                "--api-key" => {
                    api_key =
                        Some(args.next().ok_or_else(|| anyhow!("--api-key requires a value"))?);
                }
                "--sbe-url" => {
                    sbe_url =
                        args.next().ok_or_else(|| anyhow!("--sbe-url requires a value"))?;
                }
                "--json-url" => {
                    json_url =
                        args.next().ok_or_else(|| anyhow!("--json-url requires a value"))?;
                }
                "-h" | "--help" => {
                    print_help();
                }
                other => {
                    return Err(anyhow!("unknown argument: {other}\nrun with --help for usage"));
                }
            }
        }

        let symbol = symbol.to_ascii_lowercase();
        let api_key = api_key.ok_or_else(|| {
            anyhow!(
                "missing Ed25519 API Key: set BINANCE_ED25519_API_KEY env or pass --api-key"
            )
        })?;

        Ok(Self {
            sbe_url,
            json_url,
            api_key,
            symbol,
            duration_secs,
        })
    }
}

fn print_help() -> ! {
    eprintln!(
        r#"Usage:
  BINANCE_ED25519_API_KEY=... cargo run -p bookticker-benchmark -- [OPTIONS]

Options:
  --symbol <symbol>       Trading pair (lowercase), default: btcusdt
  --duration <seconds>    Collection duration in seconds, default: 60
  --api-key <key>         API Key (overrides BINANCE_ED25519_API_KEY env)
  --sbe-url <url>         SBE WS URL, default: wss://stream-sbe.binance.com:9443/ws
  --json-url <url>        JSON WS URL, default: wss://stream.binance.com:9443
  -h, --help              Show this help"#
    );
    std::process::exit(0);
}

// ---------------------------------------------------------------------------
// Event sent from WS tasks to collector
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct TickEvent {
    update_id: i64,
    recv_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Sbe,
    Json,
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env_and_args()?;
    let duration = Duration::from_secs(cfg.duration_secs);

    eprintln!(
        "=== Benchmark: {} | duration: {}s ===",
        cfg.symbol.to_ascii_uppercase(),
        cfg.duration_secs
    );
    eprintln!("SBE  -> {}", cfg.sbe_url);
    eprintln!("JSON -> {}/ws/{}@bookTicker", cfg.json_url, cfg.symbol);
    eprintln!();

    let (stop_tx, stop_rx) = watch::channel(false);
    let (tx, rx) = mpsc::unbounded_channel::<(Source, TickEvent)>();

    let sbe_tx = tx.clone();
    let json_tx = tx.clone();
    drop(tx);

    let sbe_cfg = cfg.clone();
    let sbe_stop = stop_rx.clone();
    let sbe_handle = tokio::spawn(async move { sbe_task(&sbe_cfg, sbe_tx, sbe_stop).await });

    let json_cfg = cfg.clone();
    let json_stop = stop_rx.clone();
    let json_handle = tokio::spawn(async move { json_task(&json_cfg, json_tx, json_stop).await });

    let collector_handle = tokio::spawn(collector_task(rx));

    // Wait for duration or Ctrl-C
    tokio::select! {
        _ = tokio::time::sleep(duration) => {
            eprintln!("\nDuration reached ({}s), stopping...", cfg.duration_secs);
        }
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\nCtrl-C received, stopping...");
        }
    }

    let _ = stop_tx.send(true);

    let _ = sbe_handle.await;
    let _ = json_handle.await;

    // Wait for collector to finish and print report
    match collector_handle.await {
        Ok(stats) => print_report(&cfg, &stats),
        Err(e) => eprintln!("collector error: {e:?}"),
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// SBE WebSocket task
// ---------------------------------------------------------------------------

async fn sbe_task(
    cfg: &Config,
    tx: mpsc::UnboundedSender<(Source, TickEvent)>,
    mut stop_rx: watch::Receiver<bool>,
) -> Result<()> {
    let mut req = cfg
        .sbe_url
        .as_str()
        .into_client_request()
        .context("build SBE WS request")?;
    req.headers_mut().insert(
        "X-MBX-APIKEY",
        cfg.api_key.parse().context("invalid API key header value")?,
    );

    let (mut ws, _) = connect_async(req).await.context("SBE WS connect")?;
    eprintln!("[SBE]  connected");

    let stream = format!("{}@bestBidAsk", cfg.symbol);
    let sub = format!(r#"{{"method":"SUBSCRIBE","params":["{stream}"],"id":1}}"#);
    ws.send(Message::Text(sub)).await.context("SBE subscribe")?;

    loop {
        tokio::select! {
            _ = stop_rx.changed() => {
                let _ = ws.close(None).await;
                return Ok(());
            }
            maybe_msg = ws.next() => {
                let msg = match maybe_msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return Err(anyhow!("SBE WS read: {e}")),
                    None => return Err(anyhow!("SBE WS closed")),
                };
                match msg {
                    Message::Binary(bin) => {
                        let recv_at = Instant::now();
                        if let Ok(ev) = BestBidAskStreamEvent::decode(&bin) {
                            let _ = tx.send((
                                Source::Sbe,
                                TickEvent {
                                    update_id: ev.book_update_id,
                                    recv_at,
                                },
                            ));
                        }
                    }
                    Message::Ping(payload) => {
                        let _ = ws.send(Message::Pong(payload)).await;
                    }
                    Message::Text(txt) => {
                        eprintln!("[SBE]  text: {txt}");
                    }
                    Message::Close(_) => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// JSON WebSocket task
// ---------------------------------------------------------------------------

async fn json_task(
    cfg: &Config,
    tx: mpsc::UnboundedSender<(Source, TickEvent)>,
    mut stop_rx: watch::Receiver<bool>,
) -> Result<()> {
    let url = format!("{}/ws/{}@bookTicker", cfg.json_url, cfg.symbol);
    let req = url
        .as_str()
        .into_client_request()
        .context("build JSON WS request")?;

    let (mut ws, _) = connect_async(req).await.context("JSON WS connect")?;
    eprintln!("[JSON] connected");

    loop {
        tokio::select! {
            _ = stop_rx.changed() => {
                let _ = ws.close(None).await;
                return Ok(());
            }
            maybe_msg = ws.next() => {
                let msg = match maybe_msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return Err(anyhow!("JSON WS read: {e}")),
                    None => return Err(anyhow!("JSON WS closed")),
                };
                match msg {
                    Message::Text(txt) => {
                        let recv_at = Instant::now();
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&txt) {
                            if let Some(u) = val.get("u").and_then(|v| v.as_i64()) {
                                let _ = tx.send((
                                    Source::Json,
                                    TickEvent {
                                        update_id: u,
                                        recv_at,
                                    },
                                ));
                            }
                        }
                    }
                    Message::Ping(payload) => {
                        let _ = ws.send(Message::Pong(payload)).await;
                    }
                    Message::Close(_) => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Collector: match by update_id, compute diffs
// ---------------------------------------------------------------------------

struct MatchEntry {
    sbe_at: Option<Instant>,
    json_at: Option<Instant>,
}

struct Stats {
    sbe_total: u64,
    json_total: u64,
    matched: u64,
    sbe_only: u64,
    json_only: u64,
    diffs_us: Vec<i64>,
}

async fn collector_task(mut rx: mpsc::UnboundedReceiver<(Source, TickEvent)>) -> Stats {
    let mut map: HashMap<i64, MatchEntry> = HashMap::new();
    let mut sbe_total: u64 = 0;
    let mut json_total: u64 = 0;

    while let Some((source, ev)) = rx.recv().await {
        match source {
            Source::Sbe => sbe_total += 1,
            Source::Json => json_total += 1,
        }

        let entry = map.entry(ev.update_id).or_insert(MatchEntry {
            sbe_at: None,
            json_at: None,
        });

        match source {
            Source::Sbe => entry.sbe_at = Some(ev.recv_at),
            Source::Json => entry.json_at = Some(ev.recv_at),
        }
    }

    let mut diffs_us: Vec<i64> = Vec::new();
    let mut sbe_only: u64 = 0;
    let mut json_only: u64 = 0;
    let mut matched: u64 = 0;

    for entry in map.values() {
        match (entry.sbe_at, entry.json_at) {
            (Some(sbe), Some(json)) => {
                matched += 1;
                // positive = SBE faster (JSON arrived later)
                let diff = if json >= sbe {
                    json.duration_since(sbe).as_micros() as i64
                } else {
                    -(sbe.duration_since(json).as_micros() as i64)
                };
                diffs_us.push(diff);
            }
            (Some(_), None) => sbe_only += 1,
            (None, Some(_)) => json_only += 1,
            (None, None) => {}
        }
    }

    diffs_us.sort();

    Stats {
        sbe_total,
        json_total,
        matched,
        sbe_only,
        json_only,
        diffs_us,
    }
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

fn percentile(sorted: &[i64], p: f64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p / 100.0).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn format_us(us: i64) -> String {
    let sign = if us >= 0 { "+" } else { "-" };
    let abs = us.unsigned_abs();
    let ms = abs / 1000;
    let frac = abs % 1000;
    format!("{sign}{ms}.{frac:03} ms")
}

fn print_report(cfg: &Config, stats: &Stats) {
    let sym = cfg.symbol.to_ascii_uppercase();
    println!();
    println!("========== Binance BookTicker: SBE vs JSON Latency Benchmark ==========");
    println!("Symbol:            {sym}");
    println!("Duration:          {}s", cfg.duration_secs);
    println!("SBE events:        {}", stats.sbe_total);
    println!("JSON events:       {}", stats.json_total);
    println!("Matched pairs:     {}", stats.matched);
    println!("Unmatched (SBE):   {}", stats.sbe_only);
    println!("Unmatched (JSON):  {}", stats.json_only);

    if stats.diffs_us.is_empty() {
        println!();
        println!("No matched pairs found. Cannot compute latency statistics.");
        return;
    }

    let d = &stats.diffs_us;
    let mean_us = d.iter().sum::<i64>() as f64 / d.len() as f64;

    println!();
    println!("Latency (positive = SBE faster):");
    println!("  Min:   {}", format_us(*d.first().unwrap()));
    println!("  P25:   {}", format_us(percentile(d, 25.0)));
    println!("  P50:   {}", format_us(percentile(d, 50.0)));
    println!("  P75:   {}", format_us(percentile(d, 75.0)));
    println!("  P90:   {}", format_us(percentile(d, 90.0)));
    println!("  P95:   {}", format_us(percentile(d, 95.0)));
    println!("  P99:   {}", format_us(percentile(d, 99.0)));
    println!("  Max:   {}", format_us(*d.last().unwrap()));
    println!(
        "  Mean:  {}",
        format_us(mean_us.round() as i64)
    );
    println!("=========================================================================");
}
