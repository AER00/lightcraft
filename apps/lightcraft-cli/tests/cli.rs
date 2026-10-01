//! End-to-end tests of the `lightcraft-cli` binary: `mcp` over real stdio pipes, `render`, `commands`.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_lightcraft-cli");

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("lightcraft-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

fn gradient_png(path: &std::path::Path) {
    let img = lightcraft_raster::Rgba8::from_fn(120, 80, |x, y| [(x * 2) as u8, (y * 3) as u8, 100, 255]);
    let png = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &Default::default()).unwrap();
    std::fs::write(path, png).unwrap();
}

fn mean(path: &std::path::Path) -> f64 {
    let d = lightcraft_codecs::decode(&std::fs::read(path).unwrap(), Default::default()).unwrap();
    let img = d.to_srgb8();
    img.data.iter().map(|p| (p[0] as u32 + p[1] as u32 + p[2] as u32) as f64).sum::<f64>() / (3 * img.data.len()) as f64
}

#[test]
fn mcp_over_stdio_sets_exposure_and_renders() {
    let input = tmp("in.png");
    gradient_png(&input);
    let mut child =
        Command::new(BIN).args(["mcp", input.to_str().unwrap()]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut rpc = |id: u64, method: &str, params: Value| -> Value {
        writeln!(stdin, "{}", json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["id"], id);
        v
    };
    let init = rpc(1, "initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "cli-test", "version": "0"}}));
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    let tools = rpc(2, "tools/list", json!({}));
    assert!(tools["result"]["tools"].as_array().unwrap().len() > 50);
    let base = tmp("base.png");
    let bright = tmp("bright.png");
    let r = rpc(3, "tools/call", json!({"name": "render_photo", "arguments": {"path": base.to_str().unwrap()}}));
    assert_eq!(r["result"]["isError"], false, "{r}");
    let r = rpc(4, "tools/call", json!({"name": "set_develop", "arguments": {"values": {"light.exposure": 1.0}}}));
    assert_eq!(r["result"]["isError"], false, "{r}");
    let r = rpc(5, "tools/call", json!({"name": "render_photo", "arguments": {"path": bright.to_str().unwrap()}}));
    assert_eq!(r["result"]["isError"], false, "{r}");
    assert_eq!(r["result"]["content"][0]["type"], "image");
    drop(stdin);
    assert!(child.wait().unwrap().success());
    let (a, b) = (mean(&base), mean(&bright));
    println!("cli mcp: mean sRGB exposure 0 → {a:.1}, +1 → {b:.1}");
    assert!(b > a + 10.0, "{a} → {b}");
}

#[test]
fn render_subcommand() {
    let input = tmp("r-in.png");
    gradient_png(&input);
    let plain = tmp("r-plain.png");
    let out = tmp("r-out.jpg");
    let st = Command::new(BIN).args(["render", input.to_str().unwrap(), "-o", plain.to_str().unwrap()]).status().unwrap();
    assert!(st.success());
    let st = Command::new(BIN)
        .args(["render", input.to_str().unwrap(), "-o", out.to_str().unwrap(), "--set", "light.exposure=1", "--size", "60"])
        .status()
        .unwrap();
    assert!(st.success());
    let d = lightcraft_codecs::decode(&std::fs::read(&out).unwrap(), Default::default()).unwrap();
    assert_eq!((d.width, d.height), (60, 40));
    let d = lightcraft_codecs::decode(&std::fs::read(&plain).unwrap(), Default::default()).unwrap();
    assert_eq!((d.width, d.height), (120, 80));
    assert!(mean(&out) > mean(&plain) + 10.0);
    // Bad control id fails cleanly.
    let o = Command::new(BIN).args(["render", input.to_str().unwrap(), "-o", out.to_str().unwrap(), "--set", "nope=1"]).output().unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("unknown control"));
}

#[test]
fn commands_subcommand_lists_registry() {
    let o = Command::new(BIN).args(["commands", "--json"]).output().unwrap();
    assert!(o.status.success());
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    let ids: Vec<&str> = v.as_array().unwrap().iter().filter_map(|c| c["id"].as_str()).collect();
    for id in ["develop.set", "photo.rate", "library.import", "edit.undo", "app.export"] {
        assert!(ids.contains(&id), "{id}");
    }
    let o = Command::new(BIN).arg("controls").output().unwrap();
    assert!(String::from_utf8_lossy(&o.stdout).contains("light.exposure"));
}

#[test]
fn snapshot_subcommand_renders_the_ui_headlessly() {
    let script = tmp("snap.jsonl");
    let a = tmp("snap-grid.png");
    let b = tmp("snap-export.png");
    std::fs::write(
        &script,
        format!(
            "# comment\n{}\n{}\n{}\n{}\n",
            json!({"method": "ui.set", "params": {"view": "photoGrid"}}),
            json!({"method": "ui.screenshot"}),
            json!({"method": "engine.execute", "params": {"command": "dialog.export"}}),
            json!({"method": "ui.screenshot", "params": {"path": b.to_str().unwrap()}}),
        ),
    )
    .unwrap();
    let o = Command::new(BIN)
        .args(["snapshot", "--demo", "--script", script.to_str().unwrap(), "-o", a.to_str().unwrap(), "--size", "640x400"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let replies: Vec<Value> = String::from_utf8_lossy(&o.stdout).lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(replies.len(), 4);
    assert!(replies.iter().all(|r| r["ok"] == true), "{replies:?}");
    for (p, dimmed) in [(&a, false), (&b, true)] {
        let d = lightcraft_codecs::decode(&std::fs::read(p).unwrap(), Default::default()).unwrap();
        assert_eq!((d.width, d.height), (640, 400));
        // the export dialog dims everything around it
        assert_eq!(mean(p) < mean(&a) - 2.0, dimmed, "{}", p.display());
    }
    // no script: one settled screenshot, at 2× scale
    let o = Command::new(BIN).args(["snapshot", "-o", a.to_str().unwrap(), "--size", "320x240", "--scale", "2"]).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let d = lightcraft_codecs::decode(&std::fs::read(&a).unwrap(), Default::default()).unwrap();
    assert_eq!((d.width, d.height), (640, 480));
}
