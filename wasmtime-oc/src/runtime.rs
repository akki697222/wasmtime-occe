use std::collections::VecDeque;
pub use wasmtime::{Instance, Memory, Store, Global, Table, Val, ExecutorRef};
use crate::exception::OcError;

pub const FUEL_PER_TICK: u64 = 1_000_000;

pub struct HostState {
    pub snapshot_requested: bool,
    pub pending_syscall: Option<PendingSyscall>,
    pub pending_exception: Option<OcError>,
    pub signal_queue: VecDeque<Signal>,
    pub memory_limit: usize,
}

#[derive(serde::Serialize, serde::Deserialize, bincode::Encode, bincode::Decode, Clone, Debug)]
pub struct PendingSyscall {
    pub method: String,
    pub args_ptr: i32,
    pub result_ptr: i32,
}

#[derive(serde::Serialize, serde::Deserialize, bincode::Encode, bincode::Decode, Clone, Debug)]
pub struct Signal {
    pub name: String,
    pub args: Vec<u8>, // Simplified for now, can use MessagePack later
}

pub enum StepResult {
    Yielded,
    Snapshotted,
    SyscallPending(PendingSyscall),
    Halted,
    Crashed(String),
    Completed,
}

#[derive(serde::Serialize, serde::Deserialize, bincode::Encode, bincode::Decode, Clone, Debug)]
pub enum GlobalValue {
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
}

pub struct OcComputer {
    pub store: Store<HostState>,
    pub instance: Instance,
    pub module: wasmtime::Module,
    pub memory: Memory,
    pub globals: Vec<Global>,
    pub tables: Vec<Table>,
    pub module_hash: [u8; 32],
    pub last_snapshot: Option<Vec<u8>>,
}

impl OcComputer {
    pub fn new(engine: &wasmtime::Engine, wasm_bytes: &[u8], memory_limit: usize) -> anyhow::Result<Self> {
        use wasmtime::{Module, Linker};
        let module = Module::new(engine, wasm_bytes)?;
        let mut store = Store::new(engine, HostState {
            snapshot_requested: false,
            pending_syscall: None,
            pending_exception: None,
            signal_queue: VecDeque::new(),
            memory_limit,
        });

        let mut linker = Linker::new(engine);
        crate::syscall::register_all(&mut linker)?;
        let instance = linker.instantiate(&mut store, &module)?;

        let memory = instance.get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("memory not found"))?;

        let globals = module.exports().filter_map(|e| {
            if let wasmtime::ExternType::Global(_) = e.ty() {
                instance.get_global(&mut store, e.name())
            } else {
                None
            }
        }).collect();

        let tables = module.exports().filter_map(|e| {
            if let wasmtime::ExternType::Table(_) = e.ty() {
                instance.get_table(&mut store, e.name())
            } else {
                None
            }
        }).collect();

        Ok(Self {
            store,
            instance,
            module,
            memory,
            globals,
            tables,
            module_hash: crate::snapshot::module_hash(wasm_bytes),
            last_snapshot: None,
        })
    }

    pub fn step(&mut self) -> StepResult {
        let func = self.instance.get_func(&mut self.store, "_start")
            .or_else(|| self.instance.get_func(&mut self.store, "main"))
            .unwrap_or_else(|| {
                // If no entry point, just try to call something or return error
                self.instance.exports(&mut self.store).next().and_then(|e| e.into_func()).unwrap()
            });

        // Set fuel for this tick
        let _ = self.store.set_fuel(FUEL_PER_TICK);

        match func.call(&mut self.store, &[], &mut []) {
            Ok(_) => StepResult::Completed,
            Err(e) => {
                if let Some(trap) = e.downcast_ref::<wasmtime::Trap>() {
                    if *trap == wasmtime::Trap::OutOfFuel {
                        if self.store.data().snapshot_requested {
                            self.store.data_mut().snapshot_requested = false;
                            return StepResult::Snapshotted;
                        }
                        return StepResult::Yielded;
                    }
                }
                if let Some(syscall) = self.store.data_mut().pending_syscall.take() {
                    return StepResult::SyscallPending(syscall);
                }
                StepResult::Crashed(e.to_string())
            }
        }
    }

    pub fn resume_after_syscall(&mut self, _result_bytes: &[u8], _result_ptr: i32) -> StepResult {
        // Implementation for resuming after syscall
        self.step()
    }

    pub fn request_snapshot(&mut self) {
        self.store.data_mut().snapshot_requested = true;
    }

    pub fn push_signal(&mut self, signal: Signal) {
        self.store.data_mut().signal_queue.push_back(signal);
    }

    pub fn set_pending_exception(&mut self, err: OcError) {
        self.store.data_mut().pending_exception = Some(err);
    }

    pub fn restore_from_bytes(&mut self, data: &[u8]) -> anyhow::Result<()> {
        let snap = crate::snapshot::deserialize(data)?;
        self.restore(&snap)
    }

    pub fn restore(&mut self, snap: &crate::snapshot::Snapshot) -> anyhow::Result<()> {
        // Verify module hash
        if snap.module_hash != self.module_hash {
            anyhow::bail!("module hash mismatch");
        }

        // Restore memory
        self.memory.data_mut(&mut self.store).copy_from_slice(&snap.memory);

        // Restore globals
        for (global, entry) in self.globals.iter().zip(&snap.globals) {
            let val = match entry {
                GlobalValue::I32(v) => Val::I32(*v),
                GlobalValue::I64(v) => Val::I64(*v),
                GlobalValue::F32(v) => Val::F32(*v),
                GlobalValue::F64(v) => Val::F64(*v),
            };
            global.set(&mut self.store, val).map_err(|e| anyhow::anyhow!("failed to set global: {}", e))?;
        }

        // Restore tables
        for (table, snap_table) in self.tables.iter().zip(&snap.tables) {
            for (i, &elem) in snap_table.elements.iter().enumerate() {
                if let Some(_idx) = elem {
                    // This is a simplified restoration, normally would need to find the function reference
                    // table.set(&mut self.store, i as u32, Val::FuncRef(Some(func)))?;
                }
            }
        }

        // Restore registers and PC if fork features are available
        if let Some(reg_snap) = &snap.reg_snapshot {
            match self.store.executor() {
                ExecutorRef::Interpreter(mut interp) => {
                    let pulley_vm = interp.vm();
                    unsafe {
                        pulley_vm.restore_regs(&reg_snap.clone().into());
                        if let Some(pc_offset) = snap.resume_pc_offset {
                            let bytecode_base = self.module.text().as_ptr();
                            pulley_vm.set_pc(core::ptr::NonNull::new_unchecked(bytecode_base.add(pc_offset as usize) as *mut u8));
                        }
                    }
                }
                _ => {}
            }
        }

        // Restore fuel
        let _ = self.store.set_fuel(snap.resume_fuel);

        // Restore signals
        self.store.data_mut().signal_queue = snap.pending_signals.clone().into();

        Ok(())
    }

    pub fn capture(&mut self) -> anyhow::Result<crate::snapshot::Snapshot> {
        let mut globals = Vec::new();
        for global in &self.globals {
            let val = match global.get(&mut self.store) {
                Val::I32(v) => GlobalValue::I32(v),
                Val::I64(v) => GlobalValue::I64(v),
                Val::F32(v) => GlobalValue::F32(v),
                Val::F64(v) => GlobalValue::F64(v),
                _ => anyhow::bail!("unsupported global type"),
            };
            globals.push(val);
        }

        let mut tables = Vec::new();
        for table in &self.tables {
            let mut elements = Vec::new();
            for _ in 0..table.size(&self.store) {
                // Simplified, normally would need to find function index
                elements.push(None);
            }
            tables.push(crate::snapshot::TableSnapshot { elements });
        }

        let mut reg_snapshot = None;
        let mut resume_pc_offset = None;

        match self.store.executor() {
            ExecutorRef::Interpreter(interp) => {
                let pulley_vm = interp.pulley();
                unsafe {
                    reg_snapshot = Some(pulley_vm.capture_regs().into());
                }
                // We'd need a way to get the current PC when Yielded/Suspended.
                // For now, this is a placeholder.
            }
            _ => {}
        }

        Ok(crate::snapshot::Snapshot {
            module_hash: self.module_hash,
            memory: self.memory.data(&self.store).to_vec(),
            memory_size_pages: self.memory.size(&self.store) as u32,
            globals,
            tables,
            resume_fuel: self.store.get_fuel().unwrap_or(0),
            pending_signals: self.store.data().signal_queue.iter().cloned().collect(),
            reg_snapshot,
            resume_pc_offset,
        })
    }
}
