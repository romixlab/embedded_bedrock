//! The device: [Usb] joins the EP0 state machine ([crate::control]) to the usb_v4 driver ([crate::v4]) and keeps
//! the bus status for the rest of the firmware.
//!
//! [Usb::run] is one future for the bus events and EP0, as embassy-usb's `UsbDevice::run()`: the firmware spawns
//! it in a task of its own. The class endpoints are taken from the [Driver] before it goes into [Usb::new], and
//! each class task moves its data with their `read` / `write`, not through `run()`.
//!
//! ```ignore
//! let driver = bedrock_usb::v4::embassy::new::<Desc, _, _>(p.USB, Irqs, p.PA12, p.PA11, &USB_STATE, HCLK);
//! let out = driver.bulk_out::<1, Double>();
//! let inp = driver.bulk_in::<2, Double>();
//! driver.connect();
//! let mut usb = Usb::<Desc, _, _>::new(driver, MyHandler);
//! spawner.spawn(usb_task(usb));     // usb.run().await
//! spawner.spawn(loopback(out, inp));
//! ```
//!
//! What `run()` does per event:
//! - a bus reset (the interrupt handler already set EP0 and the buffer table up again, R8): the control machine
//!   back to the default state, [Handler::reset], the status not configured, a reset after configured counted;
//! - suspend and resume: [Handler::suspended] and the status;
//! - a SETUP: the whole control transfer, packet by packet, with the [Effect]s applied before EP0 is armed again
//!   (the address after SET_ADDRESS's status stage, R14; endpoints enabled with DATA0 on SET_CONFIGURATION and
//!   SET_INTERFACE, R12; halts). A SETUP or a bus reset during a transfer ends it.

use crate::control::{Control, Effect, Ep0 as Next, Handler};
use crate::layout::RegMode;
use crate::v4::{Driver, Ep0, Ep0Error, Regs, Status, event};
use crate::{Descriptors, Direction, EndpointAddress};

/// A device `D` on the usb_v4 driver with class handlers `H` (a tuple, or `()`); `N` is the control machine's
/// class request buffer.
pub struct Usb<D: Descriptors, R: Regs + Copy, H: Handler, const N: usize = 64> {
    driver: Driver<D, R>,
    control: Control<D, N>,
    handler: H,
    ep0: Ep0<R>,
}

/// One EP0 packet: full speed's largest `bMaxPacketSize0`.
const EP0_MAX: usize = 64;

impl<D: Descriptors, R: Regs + Copy, H: Handler, const N: usize> Usb<D, R, H, N> {
    /// The device on a started `driver` ([Driver::init]); the class endpoints are taken from it first.
    pub fn new(driver: Driver<D, R>, handler: H) -> Self {
        const {
            assert!(
                D::DEVICE.max_packet_size_0 as usize <= EP0_MAX,
                "bMaxPacketSize0 is at most 64"
            )
        };
        let ep0 = driver.ep0();
        Usb {
            driver,
            control: Control::new(),
            handler,
            ep0,
        }
    }

    pub fn driver(&self) -> &Driver<D, R> {
        &self.driver
    }

    pub fn control(&self) -> &Control<D, N> {
        &self.control
    }

    pub fn handler(&mut self) -> &mut H {
        &mut self.handler
    }

    /// The bus status (also `State::status` from any task).
    pub fn status(&self) -> Status {
        self.driver.bus().status()
    }

    /// Serves the bus and EP0, forever.
    pub async fn run(&mut self) -> ! {
        loop {
            self.step().await;
        }
    }

    /// Waits for the next bus event or SETUP and handles it: bus events first, then a whole control transfer.
    /// [Usb::run] is this in a loop.
    pub async fn step(&mut self) {
        let bus = self.driver.bus();
        let events = core::future::poll_fn(|cx| {
            bus.waker.register(cx.waker());
            let e = bus.take(event::RESET | event::SUSPEND | event::RESUME);
            if e != 0 || bus.peek() & event::SETUP != 0 {
                core::task::Poll::Ready(e)
            } else {
                core::task::Poll::Pending
            }
        })
        .await;
        if events & event::RESET != 0 {
            self.control.reset(&mut self.handler);
            bus.update_status(|s| {
                s.resets = s.resets.wrapping_add(1);
                if s.configured {
                    s.resets_after_configured = s.resets_after_configured.wrapping_add(1);
                }
                s.configured = false;
                s.suspended = false;
            });
        }
        if events & (event::SUSPEND | event::RESUME) != 0 {
            // both since the last look: the order is lost, SUSPEN tells where the bus is now
            let now = self.driver.regs().cntr() & crate::v4::regs::cntr::SUSPEN != 0;
            if events & event::SUSPEND != 0 && events & event::RESUME != 0 && !now {
                self.handler.suspended(true);
            }
            self.handler.suspended(now);
            bus.update_status(|s| s.suspended = now);
        }
        if bus.peek() & event::SETUP != 0 {
            self.transfer().await;
        }
    }

    /// One control transfer from its SETUP to its status stage.
    async fn transfer(&mut self) {
        let setup = self.ep0.setup().await;
        let mut action = self.control.setup(setup, &mut self.handler);
        let mut buf = [0u8; EP0_MAX];
        loop {
            self.apply(action.effect);
            let done = match action.ep0 {
                Next::Idle => {
                    // a SETUP already in waits NAKed in its buffer until it is read
                    if self.driver.bus().peek() & event::SETUP == 0 {
                        self.ep0.idle();
                    }
                    return;
                }
                Next::Stall => {
                    self.ep0.stall();
                    return;
                }
                Next::Send { last } => {
                    let n = self.control.packet(&self.handler, &mut buf);
                    self.ep0.data_in(&buf[..n], last).await.map(|_| None)
                }
                Next::ReceiveStatus => self.ep0.status_out().await.map(|_| Some(0)),
                Next::Receive { last } => self.ep0.data_out(&mut buf, last).await.map(Some),
                Next::SendStatus => self.ep0.status_in().await.map(|_| None),
            };
            action = match done {
                // an IN packet (data or status) acknowledged
                Ok(None) => self.control.in_complete(),
                // an OUT packet (data or the zero-length status) in
                Ok(Some(n)) => self.control.out_data(&buf[..n], &mut self.handler),
                // the next SETUP or the bus reset is the next step's
                Err(Ep0Error::Setup | Ep0Error::Reset) => return,
                Err(Ep0Error::BufferOverflow) => {
                    self.ep0.stall();
                    return;
                }
            };
        }
    }

    fn apply(&mut self, effect: Effect) {
        match effect {
            Effect::None => {}
            Effect::SetAddress(a) => self.driver.set_address(a),
            Effect::Configure(configured) => {
                self.enable_where(|_| true);
                self.driver.bus().update_status(|s| {
                    s.configured = configured;
                    s.was_configured |= configured;
                });
            }
            Effect::SetInterface { interface, .. } => {
                self.enable_where(|a| D::DEVICE.endpoint_interface(a) == Some(interface))
            }
            Effect::Halt { endpoint, halted } => self.driver.set_stalled(endpoint, halted),
        }
    }

    /// Every class endpoint `filter` picks enabled when the control machine has it active, disabled otherwise,
    /// each with DATA0 (R12).
    fn enable_where(&self, filter: impl Fn(EndpointAddress) -> bool) {
        for n in 1..D::LAYOUT.count {
            let dirs: &[Direction] = match D::LAYOUT.regs[n as usize].mode {
                RegMode::Unused => &[],
                RegMode::Double { direction, .. } => match direction {
                    Direction::Out => &[Direction::Out],
                    Direction::In => &[Direction::In],
                },
                RegMode::Single { out_size, in_size } => match (out_size > 0, in_size > 0) {
                    (true, true) => &[Direction::Out, Direction::In],
                    (true, false) => &[Direction::Out],
                    (false, true) => &[Direction::In],
                    (false, false) => &[],
                },
            };
            for &d in dirs {
                let a = EndpointAddress::new(n, d);
                if filter(a) {
                    self.driver.set_enabled(a, self.control.is_active(a));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
