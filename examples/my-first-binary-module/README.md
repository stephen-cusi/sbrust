# my-first-binary-module（HL2SB sbrust）

上游同名示例的移植版：println 之外，还通过状态自身的 `print` 打招呼
（进游戏控制台和 hl2sb_lua.log 都可见）、注册 `sbrust.ping()` 做往返验证、
`gmod13_close` 卸载时自报。

## 构建

| 平台 | 命令 |
|---|---|
| Windows x64 | `cargo build --target x86_64-pc-windows-msvc --release` |
| Android arm64 | `cargo build --target aarch64-linux-android --release` |
| Android armv7 | `cargo build --target armv7-linux-androideabi --release` |

## 安装与测试

把 `target/<TARGET>/release/my_first_binary_module.dll`（.so）改名放进
`<游戏目录>/lua/bin/`：

| 产物名 | realm |
|---|---|
| `gmsv_my_first_binary_module_win64.dll` | 服务器 |
| `gmcl_my_first_binary_module_win64.dll` | 客户端 |
| `gmsv_my_first_binary_module_android64.so` | 安卓（服务器/菜单） |

游戏内：

```lua
require( "my_first_binary_module" )   -- gmod13_open：控制台打印问候
print( sbrust.ping() )                -- pong from Rust!
```

换图后状态重建，`gmod13_close` 会自报一次；再 require 或经 autorun 会重新加载。
