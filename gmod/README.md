# gmod（sbrust 核心 crate）

HL2SB 的 GMod 式二进制模块绑定层。API 面与上游 gmod-rs 对齐
（`#[gmod13_open]` / `#[gmod13_close]` / `#[lua_function]` / `State`），
符号表与常量为 Lua 5.4：差异明细见仓库根 README 的对照表与
`src/lua/import.rs` 头注释。

上游未移植的模块：`hax`（sigscan/detour，nightly 依赖）、`gmcl`
（internal_output_capture，nightly 依赖）、`net`（GMod net 库，本引擎未提供）。
