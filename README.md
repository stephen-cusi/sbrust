# ⚙ sbrust

为 **HL2SB 引擎**编写 GMod 风格二进制模块的 Rust 工具链。

Fork 自 [gmod-rs](https://github.com/WilliamVenner/gmod-rs)（MIT），ABI 从 GMod 的
LuaJIT 5.1 重定向到 HL2SB 的 **Lua 5.4.6**——引擎把 Lua 编译为共享库
`lua_shared.dll` / `liblua_shared.so`，模块在运行时从它解析 Lua C API，
与 GMod 模块从 lua_shared.dll 解析完全同构。

# 与 gmod-rs 的差异

| | gmod-rs（GMod） | sbrust（HL2SB） |
|---|---|---|
| Lua | LuaJIT（5.1 ABI） | Lua 5.4.6（引擎 lua_shared） |
| 注册表常量 | `LUA_REGISTRYINDEX = -10000`、`LUA_GLOBALSINDEX` | `LUA_REGISTRYINDEX = -1001000`，全局表走 `lua_getglobal/lua_setglobal` |
| 符号集 | `lua_pcall/lua_insert/lua_objlen/...` | `lua_pcallk/lua_rotate/lua_rawlen/...`（5.2+ 宏的底层真函数） |
| 工具链 | Nightly | **Stable**（nightly feature 已全部去除） |
| 模块目录 | `garrysmod/lua/bin/` | `<game>/lua/bin/` |
| 命名 | `gm{sv,cl}_{name}_{win32|win64|linux|linux64}.dll/.so` | 同左，另加 **`android64` / `android32`**（GMod 无安卓） |
| 入口/卸载 | `gmod13_open` / `gmod13_close` | 相同（字节级同契约） |

模块的 Lua 侧加载、命名解析、`gmod13_close` 生命周期都由引擎实现
（引擎仓 `lua/src/hl2sb_binmod.c`，随 lua_shared 导出），本仓库只关心模块侧。

# 示例

[examples/my-first-binary-module](examples/my-first-binary-module) ——
上游示例的移植版，额外注册了 `sbrust.ping()` 做双向往返验证。

# 快速开始

```sh
# Windows x64（HL2SB 桌面端）
cargo build --target x86_64-pc-windows-msvc --release -p my-first-binary-module
cp target/x86_64-pc-windows-msvc/release/my_first_binary_module.dll ^
   <游戏目录>/lua/bin/gmsv_my_first_binary_module_win64.dll

# Android arm64（HL2SB 安卓端）
cargo build --target aarch64-linux-android --release -p my-first-binary-module
# 改名 gmsv_my_first_binary_module_android64.so，放 <游戏目录>/lua/bin/
```

游戏内（服务器/客户端 realm 各自加载一次，换图自动重载）：

```lua
require( "my_first_binary_module" )
print( sbrust.ping() )   -- pong from Rust!
```

# 无游戏回归测试

`tests/host-test` 是一个独立宿主：直接加载部署好的 lua_shared、安装与引擎
完全相同的二进制模块搜索器、require 模块并做往返断言。改了 lua_shared 或
加载器，先跑它：

```sh
cargo run --target x86_64-pc-windows-msvc --release -p host-test
```

# License

MIT（继承上游 gmod-rs，© William Venner；HL2SB 5.4 移植改动同依 MIT）。
