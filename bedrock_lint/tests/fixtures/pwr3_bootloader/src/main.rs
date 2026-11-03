// a bootloader: the application it starts owns the backup domain
#[entry]
fn main() -> ! {
    let config = BootLoaderConfig::from_linkerfile_blocking(&flash, &flash, &flash);
    let bl = BootLoader::prepare::<_, _, _, 4096>(config);
    unsafe { bl.load(BASE) }
}
