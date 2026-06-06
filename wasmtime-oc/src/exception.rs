pub struct OcError {
    pub code: OcErrorCode,
    pub message: String,
}

#[repr(i32)]
pub enum OcErrorCode {
    NoComponent    = 1,
    MethodNotFound = 2,
    OutOfMemory    = 3,
    InvalidArg     = 4,
    IoError        = 5,
    Unknown        = 99,
}

pub enum GuestException {
    Halt,
    OcError { code: i32, msg_ptr: i32 },
    Other(String),
}
