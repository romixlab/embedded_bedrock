// vb135 PLAT-6: one-shot (DAR) frames on H7 are lost on an arbitration loss in the first two ID bits (ES0491 2.22.3)
fn configure(can: &mut CanConfigurator, one_shot: bool) {
    can.set_automatic_retransmit(false);
    can.set_automatic_retransmit(!one_shot);
}
