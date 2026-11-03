// Handlers of a WireWeaver server: typec_dongle FW-19 (a DAP request indexed past its end), usb_io_fw API-5 (an
// unknown mode served as the default)

pub struct ServerState {
    pins: [Pin; 8],
    rate: u32,
}

impl ServerState {
    pub(super) fn dap_request(
        &mut self,
        _cx: &mut Context<'_, impl EventOut>,
        request: RefVec<'_, u8>,
    ) -> RpcResult<u32> {
        let command = request[0];
        let count = request.get(1).copied().unwrap();
        self.transfer(command, count);
        Ok(0).into()
    }

    fn transfer(&mut self, command: u8, count: u8) {
        let words = self.buffer[count as usize];
        let _ = (command, words);
    }

    pub(super) fn gpio_mode(
        &mut self,
        _cx: &mut Context<'_, impl EventOut>,
        index_chain: [UNib32; 1],
        mode: u8,
    ) -> RpcResult<()> {
        let pin = &mut self.pins[index_chain[0].0 as usize];
        pin.mode = match mode {
            0 => Mode::Input,
            1 => Mode::Output,
            _ => Mode::Input,
        };
        let pull = Pull::try_from(mode).unwrap_or(Pull::None);
        let _ = pull;
        ().into()
    }

    pub(super) fn set_rate(
        &mut self,
        _cx: &mut Context<'_, impl EventOut>,
        hz: u32,
        data: RefVec<'_, u8>,
    ) -> RpcResult<()> {
        self.rate = 48_000_000 / hz;
        if data.len() < 4 {
            return ().into();
        }
        if data.len() > 64 {
            cnt!(rate_request_too_long);
            return ().into();
        }
        let Some(first) = data.get(0) else {
            return Err(Error::Truncated).into();
        };
        let period = self.rate * u32::from(*first);
        let _ = period;
        // bedrock-lint: allow(HOST-1) the host API limits steps to 0..=7
        let step = 1u32 << hz;
        let _ = step;
        ().into()
    }

    pub(super) fn not_yet(&mut self, _cx: &mut Context<'_, impl EventOut>, value: u8) -> RpcResult<()> {
        let checked = value.min(7);
        let _ = self.pins[checked as usize];
        todo!()
    }

    pub(super) fn safe(&mut self, _cx: &mut Context<'_, impl EventOut>, _unused: u8, mode: u8) -> RpcResult<()> {
        let m = match mode {
            0 => Mode::Input,
            1 => Mode::Output,
            _ => return Err(Error::UnknownMode).into(),
        };
        let _ = m;
        ().into()
    }
}
