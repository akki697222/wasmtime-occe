//! Implementation of the interpreter loop for Pulley with a simple `match`
//! statement.
//!
//! This module is notably in contrast to the `tail_loop.rs` which implements
//! the interpreter loop with tail calls. It's predicted that tail calls are a
//! more performant solution but that's also not available on stable Rust today,
//! so this module instead compiles on stable Rust.
//!
//! This interpreter loop is a simple `loop` with a "moral `match`" despite not
//! actually having one here. The `Decoder` API is used to dispatch to the
//! `OpVisitor` trait implementation on `Interpreter<'_>`. The literal `match`
//! is embedded within the `Decoder::decode_one` function.
//!
//! Note that as of the time of this writing there hasn't been much performance
//! analysis of this loop just yet. It's probably too simple to compile well and
//! will probably need tweaks to make it more performant.

use super::*;

impl Interpreter<'_> {
    pub fn run(mut self) -> Done {
        let mut decoder = Decoder::new();
        let mut visitor = debug::Debug(self);
        loop {
            // OC Fork B: 任意PCジャンプ
            if let Some(target) = visitor.0.state.oc_jump_target.take() {
                visitor.0.pc = unsafe { UnsafeBytecodeStream::new(target) };
            }

            // Here `decode_one` will call the appropriate `OpVisitor` method on
            // `self` via the trait implementation in the module above this.
            // That'll return whether we should keep going or exit the loop,
            // which is then done here with a conditional `break`.
            //
            // This will then continue indefinitely until the bytecode says it's
            // done. Note that only trusted bytecode is interpreted here.
            match decoder.decode_one(&mut visitor) {
                Ok(ControlFlow::Continue(())) => {}
                Ok(ControlFlow::Break(done)) => break done,
            }

            // OC Fork D: ディスパッチフック
            if visitor.0.state.oc_hook.is_some() {
                let countdown = &mut visitor.0.state.oc_hook_countdown;
                if *countdown == 0 {
                    *countdown = visitor.0.state.oc_hook_interval;
                    let hook = visitor.0.state.oc_hook.unwrap();
                    if hook(visitor.0.state) == OcHookAction::Suspend {
                        let resume = visitor.0.pc.as_ptr();
                        visitor.0.state.done_reason =
                            Some(DoneReason::SuspendedByHook { resume });
                        break Done::new();
                    }
                } else {
                    visitor.0.state.oc_hook_countdown -= 1;
                }
            }
        }
    }
}
