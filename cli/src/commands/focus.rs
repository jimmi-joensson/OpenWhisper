//! `openwhisper focus`: print which app is frontmost right now
//! (TASK-94). The same `core::focus::current()` call the desktop app
//! uses to record which app a dictation went into.

use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Args as ClapArgs;
use openwhisper_core::focus;

#[derive(ClapArgs, Debug)]
pub struct FocusArgs {
    /// Wait this long before reading, so you can switch to another
    /// app first. Without it the frontmost app is the terminal.
    #[arg(long, value_name = "MS", default_value_t = 0)]
    delay_ms: u64,
}

pub fn run(args: FocusArgs, json: bool) -> Result<()> {
    if args.delay_ms > 0 {
        eprintln!("reading in {} ms, switch to the app now", args.delay_ms);
        std::thread::sleep(Duration::from_millis(args.delay_ms));
    }
    let started = Instant::now();
    let ctx = focus::current();
    let read_us = started.elapsed().as_micros();

    if json {
        let mut value = serde_json::to_value(&ctx)?;
        value["read_us"] = serde_json::json!(read_us);
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    if ctx.is_empty() {
        println!("No frontmost app could be read.");
        return Ok(());
    }
    let or_dash = |s: &str| if s.is_empty() { "-".to_string() } else { s.to_string() };
    println!("app     {}", or_dash(&ctx.app_name));
    println!("id      {}", or_dash(&ctx.app_id));
    println!("window  {}", or_dash(&ctx.window_title));
    println!("pid     {}", ctx.pid);
    if ctx.window_title.is_empty() && cfg!(target_os = "macos") {
        eprintln!(
            "no window title: on macOS it needs the Accessibility permission for the program running this command"
        );
    }
    eprintln!("read in {:.1} ms", read_us as f64 / 1000.0);
    Ok(())
}
