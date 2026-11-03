//! The checks, one file per catalogue id. Each declares its [`CheckMeta`](crate::CheckMeta) and submits itself to the
//! registry; `tests/fixtures/` holds a small firmware per check with the findings it must give.

mod exe_3;
mod stk_1;
mod stk_2;
mod stk_4;
