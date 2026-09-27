# gmod-macros（sbrust）

`#[gmod13_open]` / `#[gmod13_close]` / `#[lua_function]` 过程宏，
与上游 gmod-rs 同名同形：展开为 `#[no_mangle] extern "C-unwind"` 导出，
open 前自动加载 lua_shared 符号表并捕获 lua_State。除上游 gmcl 分支外未改动。
