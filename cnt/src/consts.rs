/// Counters buffer size (default: 64 words = 256 bytes).
///
/// Can be customized by setting the `CNT_BUFFER_SIZE_WORDS` environment variable.
/// Use a power of 2 for best performance.
pub(crate) const BUF_SIZE: usize = 64;
