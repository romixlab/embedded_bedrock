//! The checks, one file per catalogue id. Each declares its [`CheckMeta`](crate::CheckMeta) and submits itself to the
//! registry; `tests/fixtures/` holds a small firmware per check with the findings it must give.

mod dma_1;
mod err_3;
mod exe_3;
mod mem_10;
mod mem_3;
mod mem_6;
mod perf_2;
mod perf_3;
mod siz_1;
mod siz_5;
mod siz_6;
mod stk_1;
mod stk_2;
mod stk_3;
mod stk_4;
mod stk_5;
mod stk_6;
