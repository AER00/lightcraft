# LightCraft 简体中文界面

LightCraft 的界面支持简体中文、日语和英语。语言设置保存在 `ui.json` 的 `language` 字段中。

- 通过「编辑 → 语言」或「设置 → 一般 → 语言」切换，选择会保留到下次启动。
- 已翻译的范围：菜单、照片编辑、蒙版、裁剪、设置、导入、导出，以及主要的进度提示。
- 正文字重与半粗体都使用 SIL OFL 的 **Noto Sans CJK SC**（简体中文）与 BIZ UDPGothic（日语）。
  这些字体不在本仓库，而在 [storytold/craft-fonts](https://github.com/storytold/craft-fonts)；
  使用可选的构建输入 `CRAFT_FONTS_DIR=../craft-fonts` 时（所有正式发布版）会内嵌进来。
  不指定该输入时程序照常构建运行，但中文与日文文字没有字形，会显示为方框。
- 翻译只发生在显示层：命令 ID、照片文件名、用户输入的元数据都不做改动。
- 尚未翻译的技术性错误、补充说明和发行说明仍以英文显示。
- 已知限制：**网页版（wasm32）目前只内嵌日文字体**，因此浏览器里的中文仍显示为方框
  （单个字体文件 16 MB，加上 wasm 会超过托管方 Cloudflare 的 25 MiB 单文件上限）。
  原生桌面版不受影响。
- 日期分组标题（如 `Sunday, 20 September 2026`）沿用英文格式，尚未本地化。

## 翻译的维护

静态界面文字在 `crates/ui-egui/locales/zh-hans.json`，含运行时数值的文字在
`crates/ui-egui/locales/zh-hans-formats.json`。两者都以英文原文为键。

含数值的文字遵循以下约定：

- 调用点的英文原文由 `format!` 检查（占位符写错会编译失败），翻译再按名字或位置引用同一个值。
- **按名字**：`{n}`、`{label}`、`{total}` 等与英文原文中的名字一致。
- **按位置**：`_1`、`_2`、`_3` … 对应调用点参数的先后顺序（从 1 开始），用于调整语序，
  例如 `Added {n} photo{} to “{}”` 译为 `已将 {n} 张照片添加到“{_2}”`。
- 英文用来表示单复数的那个参数（`"s"` / `""`）在中文里**不需要写**，直接不引用即可；
  不需要 `{:.0}` 之类的空占位符。
- 译文里的占位符同样会被 `format!` 检查：写了不存在的名字就编译不过。

`LIGHTCRAFT_LANGUAGE=zh-hans lightcraft-cli snapshot ...` 可以渲染中文界面用于检查。

翻译是否完整、两种目录的键与占位符是否一致、字形是否齐全，都由
`cargo test -p lightcraft-ui-egui i18n::tests` 验证（字形检查只在指定 `CRAFT_FONTS_DIR` 时实际执行）。
