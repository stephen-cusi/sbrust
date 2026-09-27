use libloading::{Library, Symbol};
use std::ffi::{c_void, CString};

type lua_State = *mut c_void;
type LuaCFunction = unsafe extern "C-unwind" fn(lua_State) -> i32;
type KFunction = Option<unsafe extern "C-unwind" fn(lua_State, i32, isize) -> i32>;

macro_rules! sym {
    ($lib:ident, $name:literal, $t:ty) => {
        unsafe { $lib.get::<$t>(concat!($name, "\0")).expect(concat!("missing symbol ", $name)) }
    };
}

const LUA_HOST: &str = r"D:\srceng\bin\lua_shared.dll";
const GAME_DIR: &str = r"D:\srceng\hl2sb";

fn main() {
    unsafe {
        let lib = Library::new(LUA_HOST).expect("failed to load lua_shared.dll");
        let newstate: Symbol<unsafe extern "C-unwind" fn() -> lua_State> = sym!(lib, "luaL_newstate", _);
        let openlibs: Symbol<unsafe extern "C-unwind" fn(lua_State)> = sym!(lib, "luaL_openlibs", _);
        let loadstring: Symbol<unsafe extern "C-unwind" fn(lua_State, *const i8) -> i32> = sym!(lib, "luaL_loadstring", _);
        let pcallk: Symbol<unsafe extern "C-unwind" fn(lua_State, i32, i32, i32, isize, KFunction) -> i32> = sym!(lib, "lua_pcallk", _);
        let tolstring: Symbol<unsafe extern "C-unwind" fn(lua_State, i32, *mut usize) -> *const i8> = sym!(lib, "lua_tolstring", _);
        let settop: Symbol<unsafe extern "C-unwind" fn(lua_State, i32)> = sym!(lib, "lua_settop", _);
        let close: Symbol<unsafe extern "C-unwind" fn(lua_State)> = sym!(lib, "lua_close", _);
        let install: Symbol<unsafe extern "C-unwind" fn(lua_State, *const i8)> = sym!(lib, "HL2SB_InstallBinaryModuleSearcher", _);
        let run_closes: Symbol<unsafe extern "C-unwind" fn(lua_State)> = sym!(lib, "HL2SB_RunBinaryModuleCloses", _);

        let l = newstate();
        openlibs(l);
        // Rust str has no NUL terminator; every C string crossing the boundary
        // must go through CString (luaL_loadstring strlen()s its input -- the
        // first run died parsing adjacent rodata as Lua).
        let gamedir = CString::new(GAME_DIR).unwrap();
        install(l, gamedir.as_ptr() as *const i8);
        println!("[host] lua_shared loaded, searcher installed, gamedir = {}", GAME_DIR);

        let script = r#"
local ok, err = pcall(function()
    local ret = require("my_first_binary_module")
    print("[host] require returned:", ret)
end)
if not ok then print("[host] REQUIRE FAILED:", err) end

local ok2, r2 = pcall(function() return sbrust.ping() end)
print("[host] sbrust.ping() ->", ok2, r2)
"#;
        let script_c = CString::new(script).unwrap();
        let rc = loadstring(l, script_c.as_ptr() as *const i8);
        if rc != 0 {
            let msg = tolstring(l, -1, std::ptr::null_mut());
            println!("[host] failed to load script: {}", unsafe { std::ffi::CStr::from_ptr(msg) }.to_string_lossy());
            std::process::exit(1);
        }
        let prc = pcallk(l, 0, 0, 0, 0, None);
        if prc != 0 {
            let msg = tolstring(l, -1, std::ptr::null_mut());
            println!("[host] RUNTIME ERROR: {}", unsafe { std::ffi::CStr::from_ptr(msg) }.to_string_lossy());
            settop(l, -2);
            std::process::exit(1);
        }

        run_closes(l);
        close(l);
        println!("[host] state closed -- chain verified end to end");
    }
}
