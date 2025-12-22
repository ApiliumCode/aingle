use crate::prelude::*;

/// Hides away the gross bit where we hook up integer pointers to length-prefixed guest memory
/// to serialization and deserialization, and returning things to the host, and memory allocation
/// and deallocation.
///
/// A lot of that is handled by the aingle_wasmer crates but this handles the boilerplate of
/// writing an extern function as they have awkward input and output signatures:
///
/// - requires remembering `#[no_mangle]`
/// - requires remembering pub extern "C"
/// - requires juggling GuestPtr on the input and output with the memory/serialization
/// - doesn't support Result returns at all, so breaks things as simple as `?`
///
/// This can be used directly as `map_extern!(external_facing_fn_name, internal_fn_name)` but it is
/// more idiomatic to use the `#[adk_extern]` attribute
///
/// ```ignore
/// #[adk_extern]
/// pub fn foo(foo_input: FooInput) -> ExternResult<FooOutput> {
///  // ... do stuff to respond to incoming calls from the host to "foo"
/// }
/// ```
#[macro_export]
macro_rules! map_extern {
    ( $name:tt, $f:ident, $input:ty, $output:ty ) => {
        $crate::paste::paste! {
            mod [< __ $name _extern >] {
                use super::*;

                #[no_mangle]
                pub extern "C" fn $name(guest_ptr: usize, len: usize) -> $crate::prelude::DoubleUSize {
                    // Setup tracing.
                    // @TODO feature flag this?
                    let _subscriber_guard = $crate::prelude::tracing::subscriber::set_default(
                        $crate::trace::WasmSubscriber::default()
                    );

                    // Deserialize the input from the host.
                    let extern_io: $crate::prelude::ExternIO = match $crate::prelude::host_args(guest_ptr as u32, len as u32) {
                        Ok(bytes) => $crate::prelude::ExternIO::from(bytes),
                        Err(err_ptr) => return err_ptr,
                    };
                    let inner: $input = match extern_io.decode() {
                        Ok(v) => v,
                        Err(e) => {
                            let bytes = extern_io.0;
                            $crate::prelude::error!(output_type = std::any::type_name::<$output>(), bytes = ?bytes, "{}", e);
                            return $crate::prelude::return_err_ptr(
                                $crate::prelude::WasmError::Guest(format!("Deserialization failed: {}", e))
                            );
                        }
                    };

                    // Call the function.
                    let output: $output = match super::$f(inner) {
                        Ok(v) => Ok(v),
                        Err(wasm_error) => return $crate::prelude::return_err_ptr(wasm_error),
                    };

                    // Serialize the output for the host.
                    // Note: We serialize the output directly (not wrapped in ExternIO)
                    // to avoid double-serialization. The host wraps the raw bytes in
                    // ExternIO on its side.
                    $crate::prelude::return_ptr(output.unwrap())
                }
            }
        }
    };
}

/// Every extern _must_ retern a `WasmError` in the case of failure.
pub type ExternResult<T> = Result<T, WasmError>;
