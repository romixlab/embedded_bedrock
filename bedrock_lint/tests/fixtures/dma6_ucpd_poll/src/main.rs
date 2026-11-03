struct Ucpd1;
struct Txdr;

impl Ucpd1 {
    fn txdr(&self) -> Txdr {
        Txdr
    }
}

impl Txdr {
    fn write_value(&self, _v: u8) {}
}

fn send_byte(ucpd: &Ucpd1, byte: u8) {
    ucpd.txdr().write_value(byte);
}
