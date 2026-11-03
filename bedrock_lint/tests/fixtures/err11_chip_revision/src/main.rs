// embedded_bedrock TPL-55: CHIP_ID.REVISION is 3 on both A3 and A4
fn silicon_revision() -> u8 {
    pac::SYSINFO.chip_id().read().revision()
}
