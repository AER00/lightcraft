# LightCraft MCP server

`lightcraft-cli mcp` exposes LightCraft to AI agents through the
[Model Context Protocol](https://modelcontextprotocol.io): newline-delimited JSON-RPC 2.0 over
stdio (protocol revision `2025-06-18`; `2025-03-26` and `2024-11-05` clients are accepted). The
server lives in `crates/mcp` (`lightcraft-mcp`, layer L5) and is hand-written: no async runtime,
no C dependencies.

It runs in one of two modes:

| Mode | Command | What it drives |
|---|---|---|
| **Headless** (default) | `lightcraft-cli mcp [--demo] [FILES/FOLDERS…]` | An in-process engine `Session`. Develop, render and export without a window. |
| **Connect** | `lightcraft-cli mcp --connect [127.0.0.1:7980]` | A running desktop app started with `lightcraft --control 7980`, through its loopback JSON-lines control channel ([control-protocol.md](control-protocol.md)). Adds the UI tools (screenshot, clicks, keys, pointer gestures). |

Options: `--demo` starts the headless session with the procedurally generated demo library;
`--compact` lists only the helper tools (see below). In connect mode the server starts even when
the app is not running yet and connects on the first call (and reconnects if the app restarts).

Logs go to stderr; stdout carries only protocol messages.

## Wiring it into a client

Build once: `cargo build --release -p lightcraft-cli` (binary: `target/release/lightcraft-cli`).

### Claude Code

```sh
# headless, with a folder of photos imported at start
claude mcp add lightcraft -- /path/to/lightcraft/target/release/lightcraft-cli mcp ~/Pictures/shoot

# or: drive the running desktop app (start it with `lightcraft --control 7980`)
claude mcp add lightcraft-app -- /path/to/lightcraft/target/release/lightcraft-cli mcp --connect 127.0.0.1:7980
```

Or check a project-scoped `.mcp.json` into your repo:

```json
{
  "mcpServers": {
    "lightcraft": {
      "command": "/path/to/lightcraft/target/release/lightcraft-cli",
      "args": ["mcp", "--connect", "127.0.0.1:7980"]
    }
  }
}
```

### Other clients (Claude Desktop, Cursor, …)

Every stdio MCP client takes the same shape: a `command` plus `args`. For example
`claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "lightcraft": {
      "command": "/path/to/lightcraft/target/release/lightcraft-cli",
      "args": ["mcp", "--demo"]
    }
  }
}
```

During development you can also point the client at `cargo run --release -p lightcraft-cli -- mcp`
(with `"cwd"` set to the repository), at the cost of a slower start.

## Tools

### Helpers

| Tool | Does |
|---|---|
| `list_commands {filter?}` | Every command: id, label, menu, shortcut, parameter doc, enabled now |
| `run_command {command, params?}` | Run any command by id |
| `import {paths, album?}` | Import files/folders (folders are scanned recursively); the first new photo becomes active |
| `query_photos {filter?, sort?, offset?, limit?}` | Photos in the current view (or matching a catalog `Filter`) |
| `select_photos {ids, active?, mode?}` | Set the selection / active photo |
| `list_controls {section?}` | Every develop slider: id (`light.exposure`…), range, default, current value |
| `get_develop {id?}` | Full develop-settings JSON |
| `set_develop {id?, values?, settings?, label?}` | `values`: `{controlId: number}`; `settings`: partial develop JSON deep-merged. Undoable |
| `apply_preset {preset, amount?, ids?}` | Apply a preset (ids from `cmd_presets_list`) |
| `crop {id?, rect?, angle?, reset?}` | Normalized crop rect `[x0,y0,x1,y1]` and straighten angle |
| `render_photo {id?, size?, format?, path?}` | Render with current settings → **image content** (PNG, or JPEG with `format: "jpeg"`), long edge `size` (default 1024) |
| `export {path, id?, longEdge?, quality?}` | Full-quality render to `.png` / `.jpg` / `.tif` / `.webp` (headless; the desktop app writes PNG) |

Tools taking `id` make that photo active first; without it they act on the active photo.

### Connect mode only

| Tool | Does |
|---|---|
| `screenshot {maxSize?, format?, path?}` | The app window as an image, after pending renders finish |
| `inspect_ui` | View, panel, window/image rects, selection, status |
| `set_ui {state}` | Merge UI state, e.g. `{"view": "detail"}` |
| `list_widgets {filter?}` / `click {widget \| x,y, count?}` | Widgets by automation id; real egui clicks |
| `press_key {key, cmd?, shift?, alt?}` / `type_text {text}` | Keyboard input (shortcuts) |
| `pointer_gesture {events}` | Gestures in normalized image coordinates (brush strokes, gradients, crop handles) |

In headless mode these return a tool error explaining how to start the app.

### One tool per command

Everything is a command in LightCraft, so `tools/list` also contains one tool per entry of the
command registry (engine commands, plus the app's UI commands such as `view.detail` when
connected): the id with `.` replaced by `_` and a `cmd_` prefix — `photo.rate` → `cmd_photo_rate`,
`develop.set` → `cmd_develop_set`, `edit.undo` → `cmd_edit_undo`. Arguments are the command's
JSON params (documented in each tool's description and by `list_commands`). Pass `--compact` to
leave these out (about 90 headless / 140 connected) when a client struggles with many tools;
`run_command` still reaches every command.

## Resources

`resources/list` / `resources/read` serve JSON snapshots:

| URI | Content |
|---|---|
| `lightcraft://library` | Source, filter, sort, selection, undo/redo labels (`library.state`) |
| `lightcraft://photos` | Photos in the current view (`catalog.query`) |
| `lightcraft://photo/active` | Everything about the active photo (`photo.inspect`) |
| `lightcraft://develop/active` | The active photo's develop settings (`develop.get`) |
| `lightcraft://controls` | Every develop control with its current value (`develop.controls`) |

## Example session

```text
→ {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"me","version":"1"}}}
← {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{…},"resources":{…}},"serverInfo":{"name":"lightcraft",…},"instructions":"…"}}
→ {"jsonrpc":"2.0","method":"notifications/initialized"}
→ {"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"set_develop","arguments":{"values":{"light.exposure":0.7}}}}
← {"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"{…}"}],"isError":false,"structuredContent":{"ok":true,"controls":[{"id":"light.exposure","value":0.7}]}}}
→ {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"render_photo","arguments":{"size":768}}}
← {"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"image","data":"iVBORw0…","mimeType":"image/png"},{"type":"text","text":"{\"id\":21,\"width\":512,\"height\":768}"}],"isError":false}}
```

Errors from commands (unknown control, nothing selected, app not reachable) come back as tool
results with `isError: true` so the model can read and correct them; malformed JSON-RPC gets the
standard error codes (-32700 parse, -32600 invalid request, -32601 method not found, -32602
invalid params, -32002 resource not found).

## Other CLI subcommands

```sh
lightcraft-cli render in.dng -o out.jpg --set light.exposure=0.5 --set light.contrast=20 --size 2048
lightcraft-cli render in.jpg -o out.png --settings look.json --preset <presetId>
lightcraft-cli commands [--json]   # the command registry
lightcraft-cli controls [--json]   # develop control ids and ranges
```

## Tests

- `crates/mcp/tests/e2e.rs` — M0.9 acceptance: over the stdio framing, set exposure and render;
  checks the decoded PNG gets brighter/darker. Runs headless and through the TCP transport
  (`Remote`) against a stand-in control server; also import → render → JPEG export of a real file.
- `apps/lightcraft-cli/tests/cli.rs` — spawns `lightcraft-cli mcp` with real pipes; `render`;
  `commands`.
