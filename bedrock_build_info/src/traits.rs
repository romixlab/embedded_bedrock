use wire_weaver::ww_trait;

#[ww_trait]
trait BuildInfo {
    /// [BedrockBuildInfo](crate::BedrockBuildInfo) with some of the fields omitted to save FLASH space
    fn compact() -> RefVec<'i, u8>;

    /// Full [BedrockBuildInfo](crate::BedrockBuildInfo)
    fn full() -> Option<RefVec<'i, u8>>;

    /// SHA256? of a firmware binary, used to get ELF from the fw registry and decode defmt and counters
    fn fw_sha() -> Option<RefVec<'i, u8>>; // TODO: use fixed array when supported?
}

// #[ww_trait]
// trait ApiRoot {
//     fn r1(r: u32);
//     ww_impl!(fw_info: BuildInfo);
// }

// ww_api!("./src/traits.rs" as bedrock_build_info::ApiRoot for Context, server = true, no_alloc = true, use_async = true, debug_to_file = "../target/traits_debug.rs");
