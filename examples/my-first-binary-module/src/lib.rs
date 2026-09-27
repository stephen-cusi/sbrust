//! HL2SB (sbrust): `my-first-binary-module`, ported from gmod-rs' example.
//!
//! The upstream example body is kept verbatim (println on open/close); on top
//! of it this build adds in-engine observable actions so the whole chain is
//! testable inside the game console:
//!
//!   1. routes a "Hello" through the state's `print` (game console +
//!      hl2sb_lua.log, instead of invisible GUI stdout),
//!   2. registers `sbrust.ping()` -- a Rust -> Lua -> Rust round trip,
//!   3. `gmod13_close` announces itself when the realm shuts down.
//!
//! Build (Windows x64):  cargo build --target x86_64-pc-windows-msvc --release
//! Install: copy target/x86_64-pc-windows-msvc/release/my_first_binary_module.dll
//!          to <game>/lua/bin/gmsv_my_first_binary_module_win64.dll
//!          (android64: rename to gmsv_my_first_binary_module_android64.so)
//! Test:    lua_run require( "my_first_binary_module" )
//!          lua_run print( sbrust.ping() )

#[macro_use]
extern crate gmod;

use gmod::lua::State;

/// Registered as `sbrust.ping()` -- proves Rust functions survive the trip
/// through Lua and can return values.
#[lua_function]
fn sbrust_ping(lua: State) -> i32 {
	unsafe {
		lua.push_string("pong from Rust!");
	}
	1
}

#[gmod13_open]
fn gmod13_open(lua: State) -> i32 {
	unsafe {
		// Upstream example behaviour: visible on a dedicated/server stdout.
		println!("Hello from binary module!");

		// HL2SB: the in-game console never sees GUI stdout -- go through the
		// state's own `print` so the greeting lands where the tester looks.
		lua.get_global(lua_string!("print"));
		lua.push_string("Hello from sbrust binary module! (gmod13_open ran)");
		lua.call(1, 0);

		// Register the `sbrust` table with ping().
		lua.get_global(lua_string!("sbrust"));
		if lua.is_nil(-1) {
			lua.pop();
			lua.new_table();
		}
		lua.push_function(sbrust_ping);
		lua.set_field(-2, lua_string!("ping"));
		lua.set_global(lua_string!("sbrust"));
	}

	// Upstream contract: number of values returned to require (none).
	0
}

#[gmod13_close]
fn gmod13_close(lua: State) -> i32 {
	unsafe {
		println!("Goodbye from binary module!");

		lua.get_global(lua_string!("print"));
		lua.push_string("sbrust: gmod13_close ran (module unloaded cleanly)");
		lua.call(1, 0);
	}

	0
}
