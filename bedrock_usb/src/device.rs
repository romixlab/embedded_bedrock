//! The device: [Usb] joins the EP0 state machine ([crate::control]) to a driver and keeps the bus status for the
//! rest of the firmware. A driver is a [Backend]: the usb_v4 driver ([crate::v4]) or the Synopsys OTG driver
//! ([crate::otg]); both hand [Usb] the same bus events and an EP0 with the same steps ([ControlEndpoint]).
//!
//! [Usb::run] is one future for the bus events and EP0, as embassy-usb's `UsbDevice::run()`: the firmware spawns
//! it in a task of its own. The class endpoints are taken from the driver before it goes into [Usb::new], and
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
//!   (the address after SET_ADDRESS's status stage, R14, or before it where the peripheral wants it so,
//!   [Backend::ADDRESS_BEFORE_STATUS]; endpoints enabled with DATA0 on SET_CONFIGURATION and SET_INTERFACE, R12;
//!   halts). A SETUP or a bus reset during a transfer ends it.

#![allow(async_fn_in_trait)]

use crate::control::{Control, Effect, Ep0 as Next, Handler};
use crate::desc::Endpoints;
use crate::v4::state::BusState;
use crate::v4::{Status, event};
use crate::{Descriptors, EndpointAddress, Recipient, RequestKind, Setup, Speed, request};

/// Why an EP0 transfer ended early.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Ep0Error {
    /// A new SETUP arrived: the transfer is over, [ControlEndpoint::setup] gives the new one.
    Setup,
    /// A bus reset.
    Reset,
    /// An OUT data packet longer than the buffer given.
    BufferOverflow,
}

/// Endpoint 0 as [Usb] drives it: the steps of a control transfer, each armed as the control machine's [Next]
/// says and awaited until the host has done its part. A SETUP or a bus reset ends a wait with an [Ep0Error].
pub trait ControlEndpoint {
    /// Waits for the next SETUP packet.
    async fn setup(&mut self) -> Setup;
    /// Sends one packet of an IN data stage and waits until the host has it; `last`: the data stage ends with it.
    async fn data_in(&mut self, data: &[u8], last: bool) -> Result<(), Ep0Error>;
    /// The zero-length OUT status stage after an IN data stage.
    async fn status_out(&mut self) -> Result<(), Ep0Error>;
    /// Receives one packet of an OUT data stage into `buf`, its length returned.
    async fn data_out(&mut self, buf: &mut [u8], last: bool) -> Result<usize, Ep0Error>;
    /// The zero-length IN status stage of a request without data or after an OUT data stage.
    async fn status_in(&mut self) -> Result<(), Ep0Error>;
    /// Request error: both directions STALL until the next SETUP.
    fn stall(&mut self);
    /// Between transfers: ready for the next SETUP, nothing to send.
    fn idle(&mut self);
}

/// A driver [Usb] runs on: its bus events and status, EP0, the device address and the class endpoints'
/// enable and halt.
pub trait Backend {
    type Ep0: ControlEndpoint;
    /// The peripheral wants the address of SET_ADDRESS before the status stage, not after it (R14). Synopsys OTG,
    /// RM0468 Rev 3 §62.15.6 "Endpoint initialization on SetAddress command": "1. Program the OTG_DCFG register
    /// with the device address received in the SetAddress command 2. Program the core to send out a status IN
    /// packet"; the core itself keeps answering on address 0 until that status stage is over.
    const ADDRESS_BEFORE_STATUS: bool;
    fn ep0(&self) -> Self::Ep0;
    /// The share the interrupt handler raises bus events in.
    fn bus(&self) -> &'static BusState;
    fn set_address(&self, address: u8);
    /// Enables or disables a class endpoint, with DATA0 (R12); endpoints the device lacks are ignored.
    fn set_enabled(&self, endpoint: EndpointAddress, enabled: bool);
    /// Stalls a class endpoint or clears its halt, the latter with DATA0 (R12).
    fn set_stalled(&self, endpoint: EndpointAddress, stalled: bool);
    /// Whether the bus is suspended now (after a SUSPEND and a RESUME raised together the order is lost).
    fn is_suspended(&self) -> bool;
    /// The speed of the last enumeration: full speed until a high-speed host has finished the chirp.
    fn speed(&self) -> Speed;
}

/// A device `D` on driver `B` with class handlers `H` (a tuple, or `()`); `N` is the control machine's class
/// request buffer.
pub struct Usb<D: Descriptors, B: Backend, H: Handler, const N: usize = 64> {
    driver: B,
    control: Control<D, N>,
    handler: H,
    ep0: B::Ep0,
}

/// One EP0 packet: `bMaxPacketSize0` is at most 64, and exactly 64 at high speed (USB 2.0 §5.5.3).
const EP0_MAX: usize = 64;

impl<D: Descriptors, B: Backend, H: Handler, const N: usize> Usb<D, B, H, N> {
    /// The device on a started `driver`; the class endpoints are taken from it first.
    pub fn new(driver: B, handler: H) -> Self {
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

    pub fn driver(&self) -> &B {
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
            let now = self.driver.is_suspended();
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
        self.control.set_speed(self.driver.speed());
        let mut action = self.control.setup(setup, &mut self.handler);
        if B::ADDRESS_BEFORE_STATUS
            && setup.request == request::SET_ADDRESS
            && setup.kind() == RequestKind::Standard
            && setup.recipient() == Recipient::Device
            && action.ep0 == Next::SendStatus
        {
            // the control machine accepted it; the Effect after the status stage writes the same address again
            self.driver.set_address(setup.value as u8 & 0x7f);
        }
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
    /// each with DATA0 (R12). An endpoint in several alternate settings is visited once per setting; the driver
    /// takes the same answer each time.
    fn enable_where(&self, filter: impl Fn(EndpointAddress) -> bool) {
        let mut it = Endpoints::new(&D::DEVICE);
        while let Some((_, ep)) = it.next() {
            if filter(ep.address) {
                self.driver
                    .set_enabled(ep.address, self.control.is_active(ep.address));
            }
        }
    }
}

#[cfg(test)]
mod tests;
