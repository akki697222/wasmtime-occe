use serde::{Serialize, Deserialize};
use pulley_interpreter::interp::RegSnapshot;
use crate::runtime::{Signal, GlobalValue};

#[derive(Serialize, Deserialize, bincode::Encode, bincode::Decode, Debug)]
pub struct Snapshot {
    pub module_hash: [u8; 32],
    pub memory: Vec<u8>,
    pub memory_size_pages: u32,
    pub globals: Vec<GlobalValue>,   // I32/I64/F32/F64
    pub tables: Vec<TableSnapshot>,
    pub resume_fuel: u64,
    pub pending_signals: Vec<Signal>,
    // OC Forkで追加:
    pub reg_snapshot: Option<RegSnapshotOc>, // フォーク版のみ有効
    pub resume_pc_offset: Option<u64>,     // bytecodeベースからのオフセット
}

#[derive(Serialize, Deserialize, bincode::Encode, bincode::Decode, Debug)]
pub struct TableSnapshot {
    pub elements: Vec<Option<u32>>,
}

#[derive(Serialize, Deserialize, bincode::Encode, bincode::Decode, Clone, Debug)]
pub struct RegSnapshotOc {
    pub x_regs: Vec<u64>,
    pub f_regs: Vec<u64>,
    pub fp_offset: isize,
    pub lr_offset: isize,
    pub stack_used_bytes: usize,
    pub stack_data: Vec<u8>,
}

impl From<RegSnapshot> for RegSnapshotOc {
    fn from(snap: RegSnapshot) -> Self {
        Self {
            x_regs: snap.x_regs,
            f_regs: snap.f_regs,
            fp_offset: snap.fp_offset,
            lr_offset: snap.lr_offset,
            stack_used_bytes: snap.stack_used_bytes,
            stack_data: snap.stack_data,
        }
    }
}

impl Into<RegSnapshot> for RegSnapshotOc {
    fn into(self) -> RegSnapshot {
        RegSnapshot {
            x_regs: self.x_regs,
            f_regs: self.f_regs,
            fp_offset: self.fp_offset,
            lr_offset: self.lr_offset,
            stack_used_bytes: self.stack_used_bytes,
            stack_data: self.stack_data,
        }
    }
}

pub fn serialize(snap: &Snapshot) -> anyhow::Result<Vec<u8>> {
    let encoded = bincode::encode_to_vec(snap, bincode::config::standard())
        .map_err(|e| anyhow::anyhow!("bincode encode failed: {}", e))?;
    Ok(lz4_flex::compress_prepend_size(&encoded))
}

pub fn deserialize(data: &[u8]) -> anyhow::Result<Snapshot> {
    let decoded = lz4_flex::decompress_size_prepended(data)
        .map_err(|e| anyhow::anyhow!("lz4 decompress failed: {}", e))?;
    let (snap, _) = bincode::decode_from_slice::<Snapshot, _>(&decoded, bincode::config::standard())
        .map_err(|e| anyhow::anyhow!("bincode decode failed: {}", e))?;
    Ok(snap)
}

pub fn module_hash(wasm_bytes: &[u8]) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(wasm_bytes);
    hasher.finalize().into()
}
