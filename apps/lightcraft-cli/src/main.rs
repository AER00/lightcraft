//! `lightcraft-cli`: headless LightCraft.
//!
//! ```text
//! lightcraft-cli mcp [--connect [ADDR]] [--demo] [--compact] [FILES/FOLDERS…]
//! lightcraft-cli render <in> -o <out> [--set control=value]… [--settings FILE.json] [--preset ID] [--size N] [--quality Q]
//! lightcraft-cli commands [--json]
//! lightcraft-cli controls [--json]
//! ```

use std::io::{BufReader, Write};
use std::path::Path;
use std::process::ExitCode;

use lightcraft_engine::Session;
use lightcraft_mcp::{Backend, DEFAULT_ADDR, Headless, Remote, Server, expand_paths, write_image};
use serde_json::{Value, json};

const USAGE: &str = "\
lightcraft-cli — headless LightCraft (photo library + raw developer)

USAGE:
  lightcraft-cli mcp [OPTIONS] [FILES/FOLDERS…]
      MCP server (JSON-RPC 2.0 over stdio). Headless by default: an in-process session with the
      given files imported. Options:
        --connect [ADDR]  drive a running app instead (`lightcraft --control 7980`; default 127.0.0.1:7980)
        --demo            headless: start with the procedurally generated demo library
        --compact         list only the helper tools (every command stays reachable via run_command)
  lightcraft-cli render <IN> -o <OUT> [OPTIONS]
      Develop one file and write the result (.png, .jpg, .tif or .webp by extension). Options:
        --set CONTROL=VALUE  set a develop slider, repeatable (e.g. --set light.exposure=0.5)
        --settings FILE      merge a partial develop-settings JSON file
        --preset ID          apply a preset (see `commands`/presets.list)
        --size N             long edge in pixels (default: full size)
        --quality Q          JPEG quality 1..100 (default 92)
  lightcraft-cli commands [--json]   list every command id with its parameters
  lightcraft-cli controls [--json]   list every develop control id with its range
  lightcraft-cli --version | --help
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("mcp") => mcp(&args[1..]),
        Some("render") => render(&args[1..]),
        Some("commands") => commands(&args[1..]),
        Some("controls") => controls(&args[1..]),
        Some("--version" | "-V" | "version") => {
            println!("lightcraft-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--help" | "-h" | "help") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown subcommand `{other}`\n\n{USAGE}")),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lightcraft-cli: {e}");
            ExitCode::FAILURE
        }
    }
}

fn mcp(args: &[String]) -> Result<(), String> {
    let mut connect: Option<String> = None;
    let mut demo = false;
    let mut compact = false;
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--connect" => {
                // Optional address argument.
                match args.get(i + 1).filter(|a| !a.starts_with("--") && a.contains(':')) {
                    Some(a) => {
                        connect = Some(a.clone());
                        i += 1;
                    }
                    None => connect = Some(DEFAULT_ADDR.to_string()),
                }
            }
            a if a.starts_with("--connect=") => connect = Some(a["--connect=".len()..].to_string()),
            "--headless" => connect = None,
            "--demo" => demo = true,
            "--compact" => compact = true,
            a if a.starts_with("--") => return Err(format!("unknown option `{a}`")),
            f => files.push(f.to_string()),
        }
        i += 1;
    }
    let backend: Box<dyn Backend> = match connect {
        Some(addr) => {
            if !files.is_empty() || demo {
                return Err("FILES and --demo apply to headless mode only (import through the `import` tool instead)".into());
            }
            match Remote::connect(&addr) {
                Ok(r) => {
                    eprintln!("lightcraft-cli mcp: connected to LightCraft at {addr}");
                    Box::new(r)
                }
                Err(e) => {
                    eprintln!("lightcraft-cli mcp: LightCraft is not reachable at {addr} yet ({e}); will retry on each call");
                    Box::new(Remote::lazy(&addr))
                }
            }
        }
        None => {
            let mut h = if demo { Headless::demo() } else { Headless::default() };
            if !files.is_empty() {
                let paths = expand_paths(&files);
                let r = h.session.execute("library.import", &json!({"paths": paths})).map_err(|e| e.to_string())?;
                eprintln!("lightcraft-cli mcp: imported {} photo(s)", r["imported"].as_array().map_or(0, Vec::len));
            }
            Box::new(h)
        }
    };
    eprintln!("lightcraft-cli mcp: serving MCP on stdio ({})", backend.describe());
    let mut server = Server::new(backend).with_command_tools(!compact);
    let stdin = std::io::stdin();
    server.serve(BufReader::new(stdin.lock()), std::io::stdout().lock()).map_err(|e| e.to_string())
}

fn take_value<'a>(args: &'a [String], i: &mut usize, flag: &str) -> Result<&'a str, String> {
    *i += 1;
    args.get(*i).map(String::as_str).ok_or_else(|| format!("{flag} needs a value"))
}

fn render(args: &[String]) -> Result<(), String> {
    let mut input = None;
    let mut output = None;
    let mut values = serde_json::Map::new();
    let mut settings: Option<Value> = None;
    let mut preset = None;
    let mut size: Option<u64> = None;
    let mut quality = 92u8;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => output = Some(take_value(args, &mut i, "-o")?.to_string()),
            "--set" => {
                let kv = take_value(args, &mut i, "--set")?;
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("--set expects control=value, got `{kv}`"))?;
                let v: f64 = v.trim().parse().map_err(|_| format!("--set {k}: `{v}` is not a number"))?;
                values.insert(k.trim().to_string(), json!(v));
            }
            "--settings" => {
                let p = take_value(args, &mut i, "--settings")?;
                let text = std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?;
                settings = Some(serde_json::from_str(&text).map_err(|e| format!("{p}: {e}"))?);
            }
            "--preset" => preset = Some(take_value(args, &mut i, "--preset")?.to_string()),
            "--size" => size = Some(take_value(args, &mut i, "--size")?.parse().map_err(|_| "--size expects a number")?),
            "--quality" => quality = take_value(args, &mut i, "--quality")?.parse().map_err(|_| "--quality expects 1..100")?,
            a if a.starts_with('-') => return Err(format!("unknown option `{a}`")),
            f if input.is_none() => input = Some(f.to_string()),
            f => return Err(format!("unexpected argument `{f}`")),
        }
        i += 1;
    }
    let input = input.ok_or("render: missing input file")?;
    let output = output.ok_or("render: missing -o OUTPUT")?;
    let mut s = Session::new().with_fs();
    let abs = expand_paths(std::slice::from_ref(&input));
    let r = s.execute("library.import", &json!({"paths": abs})).map_err(|e| e.to_string())?;
    let id = r["imported"][0].as_u64().ok_or_else(|| format!("{input}: not a readable photo"))?;
    let run = |s: &mut Session, cmd: &str, p: Value| s.execute(cmd, &p).map(|_| ()).map_err(|e| e.to_string());
    run(&mut s, "library.select", json!({"ids": [id], "active": id}))?;
    if let Some(p) = preset {
        run(&mut s, "preset.apply", json!({"id": p}))?;
    }
    if let Some(st) = settings {
        run(&mut s, "develop.merge", json!({"settings": st}))?;
    }
    if !values.is_empty() {
        run(&mut s, "develop.set", json!({"values": values}))?;
    }
    let photo = s.catalog.photo(lightcraft_engine::catalog::PhotoId(id)).ok_or("photo vanished")?;
    let full = photo.width.max(photo.height).max(1) as u64;
    let edge = size.unwrap_or(full) as usize;
    let img = s.render_now(lightcraft_engine::catalog::PhotoId(id), edge, edge)?.image;
    write_image(Path::new(&output), &img, quality)?;
    eprintln!("lightcraft-cli: wrote {output} ({}×{})", img.width, img.height);
    Ok(())
}

fn commands(args: &[String]) -> Result<(), String> {
    let mut h = Headless::demo();
    let cmds = h.call("engine.commands", json!({}))?;
    let mut out = std::io::stdout().lock();
    if args.iter().any(|a| a == "--json") {
        writeln!(out, "{}", serde_json::to_string_pretty(&cmds).unwrap_or_default()).map_err(|e| e.to_string())?;
        return Ok(());
    }
    for c in cmds.as_array().into_iter().flatten() {
        let sc = c["shortcut"].as_str().map(|s| format!("  [{s}]")).unwrap_or_default();
        writeln!(
            out,
            "{:<28} {}{sc}\n{:<28} params: {}",
            c["id"].as_str().unwrap_or(""),
            c["label"].as_str().unwrap_or(""),
            "",
            c["params"].as_str().unwrap_or("")
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn controls(args: &[String]) -> Result<(), String> {
    let mut s = Session::new();
    let v = s.execute("develop.controls", &json!({})).map_err(|e| e.to_string())?;
    let mut out = std::io::stdout().lock();
    if args.iter().any(|a| a == "--json") {
        writeln!(out, "{}", serde_json::to_string_pretty(&v).unwrap_or_default()).map_err(|e| e.to_string())?;
        return Ok(());
    }
    for c in v.as_array().into_iter().flatten() {
        writeln!(
            out,
            "{:<28} {:<22} {} .. {} (default {})",
            c["id"].as_str().unwrap_or(""),
            c["label"].as_str().unwrap_or(""),
            c["min"],
            c["max"],
            c["default"]
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
