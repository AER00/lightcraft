use serde_json::{Value, json};

use crate::{Headless, PROTOCOL_VERSION, Server, call_tool, command_tool_name};

fn server() -> Server {
    Server::new(Box::new(Headless::demo()))
}

fn rpc(s: &mut Server, id: u64, method: &str, params: Value) -> Value {
    let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
    let reply = s.handle_line(&line).expect("reply");
    let v: Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(v["jsonrpc"], "2.0");
    assert_eq!(v["id"], id);
    v
}

#[test]
fn initialize_negotiates_version() {
    let mut s = server();
    let r = rpc(&mut s, 1, "initialize", json!({"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}));
    assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(r["result"]["serverInfo"]["name"], "lightcraft");
    assert!(r["result"]["capabilities"]["tools"].is_object());
    let r = rpc(&mut s, 2, "initialize", json!({"protocolVersion": "1999-01-01"}));
    assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSION);
    assert!(s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    assert!(s.is_initialized());
}

#[test]
fn errors() {
    let mut s = server();
    let v: Value = serde_json::from_str(&s.handle_line("{not json").unwrap()).unwrap();
    assert_eq!(v["error"]["code"], -32700);
    assert_eq!(rpc(&mut s, 1, "nope", json!({}))["error"]["code"], -32601);
    assert_eq!(rpc(&mut s, 2, "tools/call", json!({}))["error"]["code"], -32602);
    assert!(s.handle_line("   ").is_none());
    // Unknown tools and failing commands are tool errors, not protocol errors.
    let r = rpc(&mut s, 3, "tools/call", json!({"name": "nope", "arguments": {}}));
    assert_eq!(r["result"]["isError"], true);
    let r = rpc(&mut s, 4, "tools/call", json!({"name": "run_command", "arguments": {"command": "no.such"}}));
    assert_eq!(r["result"]["isError"], true);
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("unknown command"));
}

#[test]
fn tools_list_has_helpers_and_every_command() {
    let mut s = server();
    let r = rpc(&mut s, 1, "tools/list", json!({}));
    let tools = r["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for n in ["list_commands", "run_command", "import", "set_develop", "render_photo", "export"] {
        assert!(names.contains(&n), "{n}");
    }
    // Headless: no UI tools.
    assert!(!names.contains(&"screenshot"));
    let hl = Headless::demo();
    for c in hl.session.commands() {
        assert!(!c.id.contains('_'), "command ids must not contain `_` ({})", c.id);
        let n = command_tool_name(c.id);
        assert!(names.contains(&n.as_str()), "{n}");
        assert!(n.len() <= 64 && n.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_'), "{n}");
    }
    assert!(names.contains(&"cmd_app_export"));
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object");
    }
    // Compact mode keeps only the helpers.
    let mut s = Server::new(Box::new(Headless::demo())).with_command_tools(false);
    let r = rpc(&mut s, 1, "tools/list", json!({}));
    assert!(r["result"]["tools"].as_array().unwrap().iter().all(|t| !t["name"].as_str().unwrap().starts_with("cmd_")));
}

#[test]
fn command_tools_run_commands() {
    let mut b = Headless::demo();
    let first = b.session.visible_cloned()[0];
    let r = call_tool(&mut b, "cmd_library_select", &json!({"ids": [first.0]}));
    assert!(!r.is_error, "{r:?}");
    let r = call_tool(&mut b, "cmd_photo_rate", &json!({"rating": 4}));
    assert!(!r.is_error, "{r:?}");
    assert_eq!(b.session.catalog.photo(first).unwrap().rating, 4);
    let r = call_tool(&mut b, "cmd_edit_undo", &json!({}));
    assert!(!r.is_error);
    assert_eq!(b.session.catalog.photo(first).unwrap().rating, 0);
}

#[test]
fn set_develop_values_and_settings() {
    let mut b = Headless::demo();
    let first = b.session.visible_cloned()[0];
    let r = call_tool(&mut b, "set_develop", &json!({"id": first.0, "values": {"light.exposure": 1.25}}));
    assert!(!r.is_error, "{r:?}");
    assert_eq!(r.structured.as_ref().unwrap()["controls"][0]["value"], 1.25);
    assert_eq!(b.session.develop_of(first).unwrap().light.exposure, 1.25);
    let r = call_tool(&mut b, "set_develop", &json!({"settings": {"light": {"contrast": 30.0}}}));
    assert!(!r.is_error, "{r:?}");
    let d = b.session.develop_of(first).unwrap();
    assert_eq!((d.light.exposure, d.light.contrast), (1.25, 30.0));
    assert!(call_tool(&mut b, "set_develop", &json!({})).is_error);
    assert!(call_tool(&mut b, "set_develop", &json!({"values": {"no.such": 1}})).is_error);
}

#[test]
fn ui_tools_need_the_app() {
    let mut b = Headless::demo();
    let r = call_tool(&mut b, "screenshot", &json!({}));
    assert!(r.is_error);
    assert!(r.content[0]["text"].as_str().unwrap().contains("--connect"));
}

#[test]
fn resources() {
    let mut s = server();
    let r = rpc(&mut s, 1, "resources/list", json!({}));
    let list = r["result"]["resources"].as_array().unwrap();
    assert_eq!(list.len(), 5);
    for (i, res) in list.iter().enumerate() {
        let uri = res["uri"].as_str().unwrap();
        let r = rpc(&mut s, 10 + i as u64, "resources/read", json!({"uri": uri}));
        let text = r["result"]["contents"][0]["text"].as_str().unwrap_or_else(|| panic!("{uri}: {r}"));
        serde_json::from_str::<Value>(text).unwrap();
    }
    assert_eq!(rpc(&mut s, 99, "resources/read", json!({"uri": "lightcraft://nope"}))["error"]["code"], -32002);
}
