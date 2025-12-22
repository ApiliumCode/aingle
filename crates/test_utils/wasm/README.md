# Wasm test utilities

This crate contains:

- several small crates that compile to Wasm and are used as test values.
- `enum TestWasm` which enumerates all of those crates.
-  `impl From<TestWasm> for SafWasm` to obtain the compiled Wasm artifacts for those crates.
- a `build.rs` file that builds all those crates for compile-time inclusion in the library.

These Wasm crates _directly_ test the host/guest implementation of AIngle without going through an ADK or other convenience interface.

We do this to make sure that it stays reasonably easy to interact with AIngle without using the `adk` and `aingle_wasmer_*` crates.

The tests that run this Wasm generally sit in the [`ribosome.rs` module in core][ribosome]. This is necessary because the Wasm crates depend on certain global functions that core defines and needs to inject.

[ribosome]: https://github.com/ApiliumCode/aingle/blob/2b83a9340fba999e8c32adb9c342bd268f0ef480/crates/aingle/src/core/ribosome.rs
