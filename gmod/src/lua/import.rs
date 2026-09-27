//! HL2SB (sbrust): Lua 5.4 import table.
//!
//! Forked from gmod-rs (which targets GMod's LuaJIT 5.1 ABI) and re-pointed at
//! HL2SB's lua_shared (Lua 5.4.6, built as a shlib by the engine).  The symbol
//! set below matches what the engine's lua_shared.dll actually exports --
//! every 5.1-only name is gone and every 5.2+-macro name resolves to its
//! underlying real function instead:
//!
//! | gmod-rs (LuaJIT 5.1)         | here (Lua 5.4)                     |
//! |------------------------------|------------------------------------|
//! | lua_pcall / lua_call         | lua_pcallk / lua_callk (ctx=0,k=0) |
//! | lua_insert/remove/replace    | lua_rotate / lua_copy              |
//! | lua_objlen                   | lua_rawlen                         |
//! | lua_tonumber / lua_tointeger | lua_tonumberx / lua_tointegerx     |
//! | lua_newuserdata              | lua_newuserdatauv (nuvalue=1)      |
//! | lua_yield (2-arg)            | lua_yieldk (ctx=0,k=0)             |
//! | lua_resume(L, narg)          | lua_resume(L, from=NULL, narg)     |
//! | lua_equal / lua_lessthan     | lua_compare (LUA_OPEQ / LUA_OPLT)  |
//! | LUA_GLOBALSINDEX/-ENVIRON    | gone: lua_getglobal / lua_setglobal|
//!
//! LUA_REGISTRYINDEX changed value with them (5.1: -10000, 5.4:
//! -LUAI_MAXSTACK-1000 = -1001000) -- raw constants here are taken from the
//! engine's lua.h, never guessed.

#[cfg(debug_assertions)]
use std::sync::atomic::AtomicI64;

use std::{cell::UnsafeCell, ffi::c_void};

use libloading::{Library, Symbol};

use super::{LuaError, State as LuaState, LuaDebug, returns::ValuesReturned};

pub type LuaInt = i64; /* 5.4 LUA_INTEGER default = long long */
pub type LuaSize = usize;
pub type LuaString = *const std::os::raw::c_char;
pub type LuaFunction = unsafe extern "C-unwind" fn(state: LuaState) -> i32;
pub type LuaNumber = f64;
pub type LuaReference = i32;
/* 5.4: lua_KContext is a ptrdiff_t, lua_KFunction a continuation fn. */
pub type LuaKContext = isize;
pub type LuaKFunction = unsafe extern "C-unwind" fn(state: LuaState, status: i32, ctx: LuaKContext) -> i32;

pub const LUA_REGISTRYINDEX: i32 = -1001000; /* -LUAI_MAXSTACK(1000000) - 1000 */

pub const LUA_MULTRET: i32 = -1;
pub const LUA_NOREF: LuaReference = -2;
pub const LUA_REFNIL: LuaReference = -1;

pub const LUA_TNONE: i32 = -1;
pub const LUA_TNIL: i32 = 0;
pub const LUA_TBOOLEAN: i32 = 1;
pub const LUA_TLIGHTUSERDATA: i32 = 2;
pub const LUA_TNUMBER: i32 = 3;
pub const LUA_TSTRING: i32 = 4;
pub const LUA_TTABLE: i32 = 5;
pub const LUA_TFUNCTION: i32 = 6;
pub const LUA_TUSERDATA: i32 = 7;
pub const LUA_TTHREAD: i32 = 8;

pub const LUA_OK: i32 = 0;
pub const LUA_YIELD: i32 = 1;
pub const LUA_ERRRUN: i32 = 2;
pub const LUA_ERRSYNTAX: i32 = 3;
pub const LUA_ERRMEM: i32 = 4;
pub const LUA_ERRERR: i32 = 5;
pub const LUA_ERRFILE: i32 = LUA_ERRERR + 1;

pub const LUA_OPEQ: i32 = 0;
pub const LUA_OPLT: i32 = 1;
pub const LUA_OPLE: i32 = 2;

pub const LUA_IDSIZE: usize = 60;
pub const LUA_RIDX_MAINTHREAD: i64 = 1;
pub const LUA_RIDX_GLOBALS: i64 = 2;

impl LuaError {
	fn get_error_message(lua_state: LuaState) -> Option<String> {
		unsafe { lua_state.get_string(-1).map(|str| str.into_owned()) }
	}

	pub(crate) fn from_lua_state(lua_state: LuaState, lua_int_error_code: i32) -> Self {
		use super::LuaError::*;
		match lua_int_error_code {
			LUA_ERRMEM => MemoryAllocationError,
			LUA_ERRERR => ErrorHandlerError,
			LUA_ERRSYNTAX | LUA_ERRRUN | LUA_ERRFILE => {
				let msg = LuaError::get_error_message(lua_state);
				match lua_int_error_code {
					LUA_ERRSYNTAX => SyntaxError(msg),
					LUA_ERRRUN => RuntimeError(msg),
					LUA_ERRFILE => FileError(msg),
					_ => unreachable!(),
				}
			}
			_ => Unknown(lua_int_error_code),
		}
	}
}

#[cfg_attr(not(debug_assertions), repr(transparent))]
pub struct LuaSharedInterface(pub(crate) UnsafeCell<*mut LuaShared>, #[cfg(debug_assertions)] AtomicI64);
impl LuaSharedInterface {
	#[cfg(debug_assertions)]
	pub(crate) fn debug_assertions(&self) {
		assert!(!unsafe { *self.0.get() }.is_null(), "The Lua state has not been initialized yet. Add `#[gmod::gmod13_open]` to your module's gmod13_open function to fix this. You can also manually set the Lua state with `gmod::set_lua_state(*mut c_void)`");

		// HL2SB: upstream used the nightly `thread_id_value` here to pin the Lua
		// state to one thread.  A stable sum over the ThreadId's Debug bytes is
		// just as unique for this purpose.
		let thread_id = format!("{:?}", std::thread::current().id());
		let thread_id = thread_id.into_bytes().iter().map(|b| *b as i64).sum();
		match self.1.compare_exchange(-1, thread_id, std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst) {
			Ok(-1) => {}, // This is the first thread to use this Lua state.
			Ok(_) => unreachable!(),
			Err(remembered_thread_id) => assert_eq!(thread_id, remembered_thread_id, "Tried to access the Lua state from another thread! The Lua state is NOT thread-safe, and should only be accessed from the main thread.")
		}
	}

	pub(super) unsafe fn load(&self) {
		*self.0.get() = Box::leak(Box::new(LuaShared::import()));
	}

	pub(super) unsafe fn set(&self, ptr: *mut c_void) {
		*self.0.get() = ptr as *mut LuaShared;
	}
}
impl std::ops::Deref for LuaSharedInterface {
	type Target = LuaShared;

	#[inline]
	fn deref(&self) -> &Self::Target {
		#[cfg(debug_assertions)]
		self.debug_assertions();

		unsafe { &**self.0.get() }
	}
}
impl std::ops::DerefMut for LuaSharedInterface {
	#[inline]
	fn deref_mut(&mut self) -> &mut Self::Target {
		#[cfg(debug_assertions)]
		self.debug_assertions();

		unsafe { &mut **self.0.get_mut() }
	}
}

pub static mut LUA_SHARED: LuaSharedInterface = LuaSharedInterface(UnsafeCell::new(std::ptr::null_mut()), #[cfg(debug_assertions)] AtomicI64::new(-1));

pub struct LuaShared {
	pub(crate) library: &'static libloading::Library,
	pub lual_newstate: Symbol<'static, unsafe extern "C-unwind" fn() -> LuaState>,
	pub lual_openlibs: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState)>,
	pub lual_loadfilex: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, path: LuaString, mode: LuaString) -> i32>,
	pub lual_loadstring: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, path: LuaString) -> i32>,
	pub lual_loadbufferx: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, buff: LuaString, sz: LuaSize, name: LuaString, mode: LuaString) -> i32>,
	pub lua_getglobal: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, name: LuaString) -> i32>,
	pub lua_setglobal: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, name: LuaString)>,
	pub lua_getfield: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32, k: LuaString) -> i32>,
	pub lua_pushvalue: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32)>,
	pub lua_pushlightuserdata: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, data: *mut c_void)>,
	pub lua_pushboolean: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, bool: i32)>,
	pub lua_tolstring: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32, out_size: *mut LuaSize) -> LuaString>,
	pub lua_pcallk: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, nargs: i32, nresults: i32, errfunc: i32, ctx: LuaKContext, k: Option<LuaKFunction>) -> i32>,
	pub lua_rotate: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32, n: i32)>,
	pub lua_copy: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, from: i32, to: i32)>,
	pub lua_gettop: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState) -> i32>,
	pub lua_type: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32) -> i32>,
	pub lua_typename: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, lua_type_id: i32) -> LuaString>,
	pub lua_setfield: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32, k: LuaString)>,
	pub lua_callk: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, nargs: i32, nresults: i32, ctx: LuaKContext, k: Option<LuaKFunction>)>,
	pub lua_createtable: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, narr: i32, nrec: i32)>,
	pub lua_settop: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, count: i32)>,
	pub lua_pushlstring: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, data: LuaString, length: LuaSize)>,
	pub lua_pushcclosure: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, func: LuaFunction, upvalues: i32)>,
	pub lua_settable: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32)>,
	pub lua_gettable: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32) -> i32>,
	pub lua_error: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState) -> i32>,
	pub lual_checkinteger: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, arg: i32) -> LuaInt>,
	pub lual_checklstring: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, arg: i32, out_size: *mut LuaSize) -> LuaString>,
	pub lual_checktype: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, arg: i32, r#type: i32)>,
	pub lua_toboolean: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32) -> i32>,
	pub lua_pushnumber: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, int: LuaNumber)>,
	pub lua_pushinteger: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, int: LuaInt)>,
	pub lua_pushnil: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState)>,
	pub lual_checknumber: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, arg: i32) -> LuaNumber>,
	pub lua_tointegerx: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32, isnum: *mut i32) -> LuaInt>,
	pub lua_tonumberx: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32, isnum: *mut i32) -> LuaNumber>,
	pub lual_checkudata: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, arg: i32, name: LuaString) -> *mut std::ffi::c_void>,
	pub lual_ref: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32) -> i32>,
	pub lual_unref: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32, r#ref: i32)>,
	pub lua_setmetatable: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32) -> i32>,
	pub lua_rawlen: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32) -> LuaSize>,
	pub lua_rawgeti: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, t: i32, index: i64)>,
	pub lua_rawseti: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, t: i32, index: i64)>,
	pub lua_getmetatable: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32) -> i32>,
	pub lua_rawequal: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, a: i32, b: i32) -> i32>,
	pub lua_compare: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, a: i32, b: i32, op: i32) -> i32>,
	pub lua_touserdata: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32) -> *mut std::ffi::c_void>,
	pub lua_getinfo: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, what: LuaString, ar: *mut LuaDebug) -> i32>,
	pub lua_getstack: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, level: i32, ar: *mut LuaDebug) -> i32>,
	pub lua_next: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32) -> i32>,
	pub lua_topointer: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32) -> *const c_void>,
	pub lua_newuserdatauv: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, size: LuaSize, nuvalue: i32) -> *mut c_void>,
	pub lual_newmetatable: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, name: LuaString) -> i32>,
	pub lua_resume: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, from: LuaState, narg: i32) -> i32>,
	pub lua_yieldk: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, nresults: i32, ctx: LuaKContext, k: Option<LuaKFunction>) -> i32>,
	pub lua_newthread: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState) -> LuaState>,
	pub lua_pushthread: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState) -> i32>,
	pub lua_tothread: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState, index: i32) -> LuaState>,
	pub lua_status: Symbol<'static, unsafe extern "C-unwind" fn(state: LuaState) -> i32>,
	pub lua_xmove: Symbol<'static, unsafe extern "C-unwind" fn(thread1: LuaState, thread2: LuaState, n: i32)>,
}
unsafe impl Sync for LuaShared {}
impl LuaShared {
	fn import() -> Self {
		unsafe {
			let (library, path) = Self::find_lua_shared();
			let library = Box::leak(Box::new(library));

			macro_rules! find_symbol {
				( $symbol:literal ) => {
					Self::find_symbol(library, concat!($symbol, "\0").as_bytes())
				};
			}

			Self {
				lual_newstate: find_symbol!("luaL_newstate"),
				lual_openlibs: find_symbol!("luaL_openlibs"),
				lua_pushlightuserdata: find_symbol!("lua_pushlightuserdata"),
				lual_loadfilex: find_symbol!("luaL_loadfilex"),
				lual_loadstring: find_symbol!("luaL_loadstring"),
				lual_loadbufferx: find_symbol!("luaL_loadbufferx"),
				lua_getglobal: find_symbol!("lua_getglobal"),
				lua_setglobal: find_symbol!("lua_setglobal"),
				lua_getfield: find_symbol!("lua_getfield"),
				lua_pushvalue: find_symbol!("lua_pushvalue"),
				lua_pushboolean: find_symbol!("lua_pushboolean"),
				lua_tolstring: find_symbol!("lua_tolstring"),
				lua_pcallk: find_symbol!("lua_pcallk"),
				lua_rotate: find_symbol!("lua_rotate"),
				lua_copy: find_symbol!("lua_copy"),
				lua_gettop: find_symbol!("lua_gettop"),
				lua_type: find_symbol!("lua_type"),
				lua_typename: find_symbol!("lua_typename"),
				lua_setfield: find_symbol!("lua_setfield"),
				lua_callk: find_symbol!("lua_callk"),
				lua_createtable: find_symbol!("lua_createtable"),
				lua_settop: find_symbol!("lua_settop"),
				lua_pushlstring: find_symbol!("lua_pushlstring"),
				lua_pushcclosure: find_symbol!("lua_pushcclosure"),
				lua_settable: find_symbol!("lua_settable"),
				lua_gettable: find_symbol!("lua_gettable"),
				lua_error: find_symbol!("lua_error"),
				lual_checkinteger: find_symbol!("luaL_checkinteger"),
				lual_checklstring: find_symbol!("luaL_checklstring"),
				lual_checktype: find_symbol!("luaL_checktype"),
				lua_toboolean: find_symbol!("lua_toboolean"),
				lua_pushnumber: find_symbol!("lua_pushnumber"),
				lua_pushinteger: find_symbol!("lua_pushinteger"),
				lua_pushnil: find_symbol!("lua_pushnil"),
				lual_checknumber: find_symbol!("luaL_checknumber"),
				lua_tointegerx: find_symbol!("lua_tointegerx"),
				lua_tonumberx: find_symbol!("lua_tonumberx"),
				lual_checkudata: find_symbol!("luaL_checkudata"),
				lual_ref: find_symbol!("luaL_ref"),
				lual_unref: find_symbol!("luaL_unref"),
				lua_setmetatable: find_symbol!("lua_setmetatable"),
				lua_rawlen: find_symbol!("lua_rawlen"),
				lua_rawgeti: find_symbol!("lua_rawgeti"),
				lua_rawseti: find_symbol!("lua_rawseti"),
				lua_getmetatable: find_symbol!("lua_getmetatable"),
				lua_rawequal: find_symbol!("lua_rawequal"),
				lua_compare: find_symbol!("lua_compare"),
				lua_touserdata: find_symbol!("lua_touserdata"),
				lua_getinfo: find_symbol!("lua_getinfo"),
				lua_getstack: find_symbol!("lua_getstack"),
				lua_next: find_symbol!("lua_next"),
				lua_topointer: find_symbol!("lua_topointer"),
				lua_newuserdatauv: find_symbol!("lua_newuserdatauv"),
				lual_newmetatable: find_symbol!("luaL_newmetatable"),
				lua_resume: find_symbol!("lua_resume"),
				lua_yieldk: find_symbol!("lua_yieldk"),
				lua_newthread: find_symbol!("lua_newthread"),
				lua_pushthread: find_symbol!("lua_pushthread"),
				lua_tothread: find_symbol!("lua_tothread"),
				lua_status: find_symbol!("lua_status"),
				lua_xmove: find_symbol!("lua_xmove"),
				library,
			}
		}
	}

	unsafe fn find_symbol<T>(library: &'static Library, name: &[u8]) -> Symbol<'static, T> {
		match library.get(name) {
			Ok(symbol) => symbol,
			Err(err) => panic!("Failed to find symbol \"{}\"\n{:#?}", String::from_utf8_lossy(name), err),
		}
	}

	/// HL2SB: the engine's lua_shared is loaded into the process before any
	/// binary module runs (client.dll/server.dll import it), so ask the loader
	/// for the already-loaded module first -- upstream's
	/// open_already_loaded("lua_shared.dll") on Windows, extended with a
	/// plain dlopen-by-soname fallback for Linux/Android where libloading has
	/// no open_already_loaded equivalent (dlopen of an already-loaded soname
	/// returns the existing handle; RTLD_NOLOAD is unavailable on old bionic).
	#[cfg(target_os = "windows")]
	unsafe fn open_loaded_lua_shared() -> Option<(Library, &'static str)> {
		libloading::os::windows::Library::open_already_loaded("lua_shared.dll")
			.ok()
			.map(|lib| (Library::from(lib), "lua_shared.dll"))
	}

	#[cfg(not(target_os = "windows"))]
	unsafe fn open_loaded_lua_shared() -> Option<(Library, &'static str)> {
		libloading::Library::open("liblua_shared.so").ok().map(|lib| (lib, "liblua_shared.so"))
	}

	#[cfg(target_os = "windows")]
	pub unsafe fn find_lua_shared() -> (Library, &'static str) {
		if let Some(loaded) = Self::open_loaded_lua_shared() {
			return loaded;
		}

		crate::open_library_raw!("lua_shared.dll")
		.expect("Failed to load lua_shared.dll (is the HL2SB engine running?)")
	}

	#[cfg(all(target_os = "android", target_pointer_width = "64"))]
	pub unsafe fn find_lua_shared() -> (Library, &'static str) {
		if let Some(loaded) = Self::open_loaded_lua_shared() {
			return loaded;
		}

		crate::open_library_raw!("liblua_shared.so")
		.expect("Failed to load liblua_shared.so (is the HL2SB engine running?)")
	}

	#[cfg(all(target_os = "android", target_pointer_width = "32"))]
	pub unsafe fn find_lua_shared() -> (Library, &'static str) {
		if let Some(loaded) = Self::open_loaded_lua_shared() {
			return loaded;
		}

		crate::open_library_raw!("liblua_shared.so")
		.expect("Failed to load liblua_shared.so (is the HL2SB engine running?)")
	}

	#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
	pub unsafe fn find_lua_shared() -> (Library, &'static str) {
		if let Some(loaded) = Self::open_loaded_lua_shared() {
			return loaded;
		}

		crate::open_library_raw!("liblua_shared.so")
		.expect("Failed to load liblua_shared.so (is the HL2SB engine running?)")
	}

	#[cfg(all(target_os = "linux", target_pointer_width = "32"))]
	pub unsafe fn find_lua_shared() -> (Library, &'static str) {
		if let Some(loaded) = Self::open_loaded_lua_shared() {
			return loaded;
		}

		crate::open_library_raw!("liblua_shared.so")
		.expect("Failed to load liblua_shared.so (is the HL2SB engine running?)")
	}
}
