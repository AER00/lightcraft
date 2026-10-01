//! `lightcraft-cli`: headless LightCraft.
//!
//! ```text
//! lightcraft-cli mcp [--connect [ADDR]] [--demo] [--compact] [FILES/FOLDERS…]
//! lightcraft-cli render <in> -o <out> [--set control=value]… [--settings FILE.json] [--preset ID] [--size N] [--quality Q]
//! lightcraft-cli snapshot [--library DIR | --demo] [--script FILE.jsonl] [-o OUT.png] [--size WxH] [--scale S] [FILES…]
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
        --library DIR     headless: open (or create) a persistent LightCraft library; edits are saved
        --compact         list only the helper tools (every command stays reachable via run_command)
  lightcraft-cli render <IN> -o <OUT> [OPTIONS]
      Develop one file and write the result (.png, .jpg, .tif or .webp by extension). Options:
        --set CONTROL=VALUE  set a develop slider, repeatable (e.g. --set light.exposure=0.5)
        --settings FILE      merge a partial develop-settings JSON file
        --preset ID          apply a preset (see `commands`/presets.list)
        --size N             long edge in pixels (default: full size)
        --quality Q          JPEG quality 1..100 (default 92)
  lightcraft-cli snapshot [OPTIONS] [FILES/FOLDERS…]
      Run the full app UI headlessly (no window, no GPU: CPU-rasterized egui) and write PNGs.
      Options:
        --demo            the procedural demo library (default unless --library or FILES)
        --library DIR     open (or create) a LightCraft library
        --script FILE     JSON-lines control-protocol requests (docs/control-protocol.md), one
                          per line: {\"method\": \"ui.set\", \"params\": {\"view\": \"detail\"}}.
                          Replies go to stdout. `ui.screenshot` without a path writes -o (then
                          OUT-2.png, OUT-3.png…); `ui.settle {timeoutMs?}` waits for renders.
        -o, --output OUT  PNG path (a final screenshot is written here if the script took none)
        --size WxH        window size in points (default 1600x1000)
        --scale S         pixels per point (default 1)
  lightcraft-cli commands [--json]   list every command id with its parameters
  lightcraft-cli controls [--json]   list every develop control id with its range
  lightcraft-cli --version | --help
";

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() -> ExitCode {
    // `--features dhat-heap`: count allocations; the profile is written when `_heap` drops
    // (LIGHTCRAFT_DHAT_FILE, default dhat-heap.json).
    #[cfg(feature = "dhat-heap")]
    let _heap = {
        let file = std::env::var("LIGHTCRAFT_DHAT_FILE").unwrap_or_else(|_| "dhat-heap.json".into());
        lightcraft_engine::memory::set_heap_stats(|| {
            let s = dhat::HeapStats::get();
            lightcraft_engine::memory::HeapUsage { current: s.curr_bytes as u64, peak: s.max_bytes as u64 }
        });
        dhat::Profiler::builder().file_name(file).build()
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("mcp") => mcp(&args[1..]),
        Some("render") => render(&args[1..]),
        Some("snapshot") => snapshot(&args[1..]),
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
    let mut library: Option<String> = None;
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
            "--library" => library = Some(take_value(args, &mut i, "--library")?.to_string()),
            a if a.starts_with("--") => return Err(format!("unknown option `{a}`")),
            f => files.push(f.to_string()),
        }
        i += 1;
    }
    let backend: Box<dyn Backend> = match connect {
        Some(addr) => {
            if !files.is_empty() || demo || library.is_some() {
                return Err("FILES, --demo and --library apply to headless mode only (import through the `import` tool instead)".into());
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
            let mut h = match &library {
                Some(dir) => {
                    let mut h = Headless::default();
                    let r = h.session.open_library(dir, demo).map_err(|e| e.to_string())?;
                    eprintln!("lightcraft-cli mcp: opened library {dir} ({r:?})");
                    h
                }
                None if demo => Headless::demo(),
                None => Headless::default(),
            };
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

fn snapshot(args: &[String]) -> Result<(), String> {
    use lightcraft_ui_egui::headless::Headless;
    use std::time::{Duration, Instant};
    let mut library: Option<String> = None;
    let mut script: Option<String> = None;
    let mut output: Option<String> = None;
    let mut size = [1600.0f32, 1000.0];
    let mut scale = 1.0f32;
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--demo" => {}
            "--library" => library = Some(take_value(args, &mut i, "--library")?.to_string()),
            "--script" => script = Some(take_value(args, &mut i, "--script")?.to_string()),
            "-o" | "--output" => output = Some(take_value(args, &mut i, "-o")?.to_string()),
            "--size" => {
                let v = take_value(args, &mut i, "--size")?;
                let (w, h) = v.split_once(['x', 'X']).ok_or("--size expects WxH, e.g. 1600x1000")?;
                size = [w.trim().parse().map_err(|_| "--size: bad width")?, h.trim().parse().map_err(|_| "--size: bad height")?];
            }
            "--scale" => scale = take_value(args, &mut i, "--scale")?.parse().map_err(|_| "--scale expects a number")?,
            a if a.starts_with('-') => return Err(format!("unknown option `{a}`")),
            f => files.push(f.to_string()),
        }
        i += 1;
    }
    if script.is_none() && output.is_none() {
        return Err("snapshot: give -o OUT.png and/or --script FILE".into());
    }
    if !(scale > 0.0 && size[0] >= 1.0 && size[1] >= 1.0) {
        return Err("snapshot: bad --size/--scale".into());
    }
    let t0 = Instant::now();
    let mut session = match &library {
        Some(dir) => {
            let mut s = Session::new().with_fs();
            s.open_library(dir, false).map_err(|e| format!("{dir}: {e}"))?;
            s
        }
        None if files.is_empty() => Session::with_demo().with_fs(),
        None => Session::new().with_fs(),
    };
    if !files.is_empty() {
        session.execute("library.import", &json!({"paths": expand_paths(&files)})).map_err(|e| e.to_string())?;
    }
    let services = lightcraft_ui_egui::Services {
        write: Some(Box::new(|p: &str, b: &[u8]| {
            if let Some(dir) = Path::new(p).parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::write(p, b).map_err(|e| format!("{p}: {e}"))
        })),
        png: Some(Box::new(|img: &lightcraft_raster::Rgba8| {
            lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(img), &lightcraft_codecs::EncodeMeta::default()).unwrap_or_default()
        })),
        ..Default::default()
    };
    let app = lightcraft_ui_egui::LightcraftApp::new(session, services);
    let mut h = Headless::new(app, size, scale);
    let timeout = Duration::from_secs(60);
    let mut shots = 0usize;
    let next_path = |shots: &mut usize| -> Option<String> {
        let out = output.as_deref()?;
        *shots += 1;
        if *shots == 1 {
            return Some(out.to_string());
        }
        let p = Path::new(out);
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ext = p.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_else(|| "png".into());
        Some(p.with_file_name(format!("{stem}-{shots}.{ext}")).to_string_lossy().to_string())
    };
    let mut wrote_output = false;
    if let Some(script) = &script {
        let text = std::fs::read_to_string(script).map_err(|e| format!("{script}: {e}"))?;
        let mut out = std::io::stdout().lock();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            let msg: Value = serde_json::from_str(line).map_err(|e| format!("{script}:{}: {e}", n + 1))?;
            let id = msg.get("id").cloned().unwrap_or(json!(n + 1));
            let method = msg.get("method").and_then(Value::as_str).unwrap_or("").to_string();
            let mut params = msg.get("params").cloned().unwrap_or(json!({}));
            let ts = Instant::now();
            let mut reply = match method.as_str() {
                "ui.settle" => {
                    let ms = params.get("timeoutMs").and_then(Value::as_u64).unwrap_or(20_000);
                    json!({"ok": true, "result": {"settled": h.settle(Duration::from_millis(ms))}})
                }
                "ui.screenshot" => {
                    if params.get("path").and_then(Value::as_str).is_none() {
                        match next_path(&mut shots) {
                            Some(p) => {
                                wrote_output = true;
                                params["path"] = json!(p);
                            }
                            None => return Err(format!("{script}:{}: ui.screenshot needs a `path` (or give -o)", n + 1)),
                        }
                    }
                    h.request(&method, params, timeout)
                }
                _ => h.request(&method, params, timeout),
            };
            if let Some(o) = reply.as_object_mut() {
                o.insert("id".into(), id);
            }
            writeln!(out, "{reply}").map_err(|e| e.to_string())?;
            if method == "ui.screenshot" {
                eprintln!(
                    "lightcraft-cli snapshot: {} ({:.0} ms)",
                    reply["result"]["path"].as_str().unwrap_or("?"),
                    ts.elapsed().as_secs_f64() * 1000.0
                );
            }
            if h.quit_requested() {
                break;
            }
        }
    }
    if !wrote_output && let Some(path) = next_path(&mut shots) {
        let r = h.request("ui.screenshot", json!({"path": path}), timeout);
        if r["ok"] != true {
            return Err(format!("screenshot failed: {}", r["error"]));
        }
        eprintln!("lightcraft-cli snapshot: wrote {path} ({}×{})", r["result"]["width"], r["result"]["height"]);
    }
    eprintln!("lightcraft-cli snapshot: done in {:.2} s ({} frames)", t0.elapsed().as_secs_f64(), h.frames());
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
