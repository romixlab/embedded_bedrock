impl ServerState {
    async fn led_set(&mut self, on: bool) {
        self.led.set(on);
    }

    async fn sensor(&mut self) -> u16 {
        unwrap!(self.adc.read().await)
    }

    // bedrock-lint: allow(EXE-12) the generated server trait wants it async
    async fn version(&mut self) -> u32 {
        1
    }
}

async fn select_both() {
    select!(a = A.wait() => a, b = B.wait() => b).await;
}

fn plain() {}
