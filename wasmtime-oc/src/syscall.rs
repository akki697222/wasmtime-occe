use wasmtime::Linker;
use crate::runtime::HostState;

pub fn register_all(linker: &mut Linker<HostState>) -> anyhow::Result<()> {
    linker.func_wrap("oc", "component_invoke", |_caller: wasmtime::Caller<'_, HostState>, _method_ptr: i32, _args_ptr: i32, _result_ptr: i32| -> i32 {
        // Implementation will be added later
        0
    })?;
    linker.func_wrap("oc", "signal_count", |caller: wasmtime::Caller<'_, HostState>| -> i32 {
        caller.data().signal_queue.len() as i32
    })?;
    linker.func_wrap("oc", "signal_pop", |_caller: wasmtime::Caller<'_, HostState>, _buf_ptr: i32| -> i32 {
        0
    })?;
    linker.func_wrap("oc", "log", |_caller: wasmtime::Caller<'_, HostState>, _msg_ptr: i32, _msg_len: i32| {
    })?;
    linker.func_wrap("oc", "checkpoint_poll", |_caller: wasmtime::Caller<'_, HostState>| {
    })?;
    linker.func_wrap("oc", "memory_size", |_caller: wasmtime::Caller<'_, HostState>| -> i32 {
        0
    })?;
    Ok(())
}
