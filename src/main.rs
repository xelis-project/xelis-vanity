use std::{sync::atomic::{AtomicUsize, Ordering}, thread, time::{Duration, Instant}};

use clap::Parser;
use lazy_static::lazy_static;
use log::{error, info, warn};
use xelis_common::{
    async_handler,
    config::{PREFIX_ADDRESS, VERSION},
    crypto::KeyPair,
    prompt::{
        Color,
        LogLevel,
        Prompt,
        PromptError,
        ShareablePrompt
    },
    serializer::Serializer,
    tokio::{self, sync::Mutex},
    utils::format_hashrate
};
use xelis_wallet::mnemonics;

#[derive(Parser)]
#[clap(version = VERSION, about = "XELIS is an innovative cryptocurrency built from scratch with BlockDAG, Homomorphic Encryption, Zero-Knowledge Proofs, and Smart Contracts.")]
#[command(styles = xelis_common::get_cli_styles())]
pub struct Config {
    /// The prefix for the address to search for
    #[clap(short, long)]
    pub prefix: String,
    /// Language index for the seed
    #[clap(short, long, default_value_t = 0)]
    pub language: usize,
    /// Numbers of threads to use (at least 1, max: 65535)
    /// By default, this will try to detect the number of threads available on your CPU.
    #[clap(short, long)]
    pub num_threads: Option<usize>,
    /// Disable the usage of colors in log
    #[clap(long)]
    disable_log_color: bool,
    /// Disable terminal interactive mode
    /// You will not be able to write CLI commands in it or to have an updated prompt
    #[clap(long)]
    disable_interactive_mode: bool,
}

static RATE_COUNTER: AtomicUsize = AtomicUsize::new(0);
lazy_static! {
    static ref RATE_LAST_TIME: Mutex<Instant> = Mutex::new(Instant::now());
}

#[tokio::main]
async fn main() {
    let config = Config::parse();
    let prompt = match Prompt::new(LogLevel::Info, "logs/", "logs.log", true, false, config.disable_log_color, !config.disable_interactive_mode, Vec::new(), LogLevel::Info) {
        Ok(value) => value,
        Err(e) => {
            error!("Couldn't initialize prompt: {}", e);
            return;
        }
    };

    let detected_threads = match thread::available_parallelism() {
        Ok(value) => value.get(),
        Err(e) => {
            warn!("Couldn't detect number of available threads: {}, fallback to 1 thread only", e);
            1
        }
    };

    let threads = match config.num_threads {
        Some(value) => value,
        None => detected_threads
    };

    if threads < 1 {
        error!("Number of threads must be at least 1");
        return;
    }

    info!("Total threads to use: {} (detected: {})", threads, detected_threads);
    info!("Searching for address with prefix: {}", config.prefix);

    let prefix = format!("{}:{}", PREFIX_ADDRESS, config.prefix);
    for i in 0..threads {
        let prefix = prefix.clone();
        // TODO: abort threads when one of them found the address
        thread::spawn(move || search_for_prefix(prefix, config.language, i));
    }

    if let Err(e) = run_prompt(prompt).await {
        error!("Error while running prompt: {}", e);
    }
}

fn search_for_prefix(prefix: String, language: usize, thread: usize) {
    loop {
        let keypair = KeyPair::new();
        let address = keypair.get_public_key()
            .to_address(true)
            .to_string();

        if address.starts_with(&prefix) {
            info!("Thread #{} found: {}", thread, address);
            info!("Private key: {}", keypair.get_private_key().to_hex());
            info!("Seed: {}", mnemonics::key_to_words(keypair.get_private_key(), language).unwrap().join(" "));
            break;
        }

        RATE_COUNTER.fetch_add(1, Ordering::Relaxed);
    }
}

async fn run_prompt(prompt: ShareablePrompt) -> Result<(), PromptError> {
    let closure = |_: &_, _: _| async {
        let rate = {
            let mut last_time = RATE_LAST_TIME.lock().await;
            let counter = RATE_COUNTER.swap(0, Ordering::Relaxed);

            let hashrate = 1000f64 / (last_time.elapsed().as_millis() as f64 / counter as f64);
            *last_time = Instant::now();

            prompt.colorize_string(Color::Green, &format!("{}", format_hashrate(hashrate)))
        };

        Ok(
            format!(
                "{} | {} {} ",
                prompt.colorize_str(Color::Blue, "XELIS Vanity"),
                rate,
                prompt.colorize_str(Color::BrightBlack, ">>")
            )
        )
    };

    prompt.start(Duration::from_secs(1), Box::new(async_handler!(closure)), None).await
}