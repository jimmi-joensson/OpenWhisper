//! `openwhisper focus`: print which app is frontmost right now
//! (TASK-94). The same `core::focus::current()` call the desktop app
//! uses to record which app a dictation went into.
//!
//! `openwhisper focus --running` lists the apps that are open
//! (`core::focus::running_apps()`): the ids a default mode can be set
//! for with `openwhisper modes app set` (TASK-96).

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
    /// List the apps that are open now, with their ids, instead of
    /// the frontmost one.
    #[arg(long, conflicts_with = "delay_ms")]
    running: bool,
}

/// `--running`: one line per open app, or a JSON array of
/// `{ "app_id", "app_name" }`.
fn running(json: bool) -> Result<()> {
    let apps = focus::running_apps();
    if json {
        let rows: Vec<serde_json::Value> = apps
            .iter()
            .map(|(id, name)| serde_json::json!({ "app_id": id, "app_name": name }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if apps.is_empty() {
        println!("No open apps could be read.");
        return Ok(());
    }
    for (id, name) in &apps {
        println!("{id:<44} {name}");
    }
    Ok(())
}

pub fn run(args: FocusArgs, json: bool) -> Result<()> {
    if args.running {
        return running(json);
    }
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
