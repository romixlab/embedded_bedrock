struct ServerState {
    i2c: I2c<'static, Async>,
    jobs: Sender<'static, CriticalSectionRawMutex, Job, 4>,
}

impl WireWeaverAsyncApiBackend for ServerState {
    async fn process_bytes(&mut self, data: &[u8]) -> Result<(), Error> {
        self.process_request_bytes(data).await
    }
}

impl ServerState {
    async fn read_sensor(&mut self, addr: u8) -> u8 {
        let mut b = [0];
        self.i2c.write_read(addr, &[0], &mut b).await.ok();
        b[0]
    }

    async fn read_sensor_deferred(&mut self, addr: u8) {
        self.jobs.send(Job::Read(addr)).await;
        stream_data_ser().i2c().done_send().await;
    }
}
