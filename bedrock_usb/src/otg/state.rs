//! What the interrupt handler and the tasks share: wakers, bus events, the last SETUP packet, the speed, the OUT
//! packet queues and the RAM buffers the interrupt handler empties the receive FIFO into. One [State] per device,
//! sized by its layout ([crate::otg_state!]).
//!
//! The receive FIFO is one for all OUT endpoints and EP0's SETUP packets (RM0468 Rev 3 §62.11.1), so a packet the
//! firmware hasn't read yet can't wait in it: the next SETUP would queue behind it. The interrupt handler copies
//! each packet into its endpoint's buffer here, and an endpoint is only armed (EPENA, CNAK) while it has a free
//! buffer, so the core NAKs the host until the firmware has read.

use core::cell::{Cell, UnsafeCell};

use critical_section::{CriticalSection, Mutex};

use crate::Speed;
use crate::v4::state::{BusState, WakerCell};

/// The OUT side of an endpoint: up to two packets received and not read, and which buffer the core fills.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct OutQueue {
    /// Packets received and not read: 0 to `packets`.
    ready: u8,
    /// The buffer of the oldest unread packet.
    head: u8,
    /// The endpoint is armed and fills buffer `(head + ready) % packets`.
    armed: bool,
    /// Bytes of the packet in each buffer.
    len: [u16; 2],
    /// Bytes the interrupt handler copied into the armed buffer, committed at XFRC.
    pending: u16,
    /// Buffers: 1 or 2; 0 while the endpoint is disabled (packets for it are dropped).
    packets: u8,
}

impl OutQueue {
    pub const DISABLED: OutQueue = OutQueue {
        ready: 0,
        head: 0,
        armed: false,
        len: [0; 2],
        pending: 0,
        packets: 0,
    };

    /// Enabled with `packets` buffers, nothing received, not armed yet.
    pub const fn new(packets: u8) -> Self {
        OutQueue {
            packets,
            ..Self::DISABLED
        }
    }

    pub const fn is_enabled(&self) -> bool {
        self.packets != 0
    }

    /// The buffer the core may fill next, when one is free and the endpoint isn't armed.
    pub const fn to_arm(&self) -> Option<usize> {
        if self.packets != 0 && !self.armed && self.ready < self.packets {
            Some(((self.head + self.ready) % self.packets) as usize)
        } else {
            None
        }
    }

    pub fn arm(&mut self) {
        debug_assert!(self.to_arm().is_some());
        self.armed = true;
        self.pending = 0;
    }

    /// The buffer an OUT data packet goes to now (the armed one).
    pub const fn filling(&self) -> Option<usize> {
        if self.armed {
            Some(((self.head + self.ready) % self.packets) as usize)
        } else {
            None
        }
    }

    /// The interrupt handler copied `len` bytes into [OutQueue::filling]'s buffer.
    pub fn copied(&mut self, len: u16) {
        self.pending = len;
    }

    /// XFRC: the packet in the armed buffer is complete. False when nothing was armed (a packet for a disabled
    /// endpoint).
    pub fn completed(&mut self) -> bool {
        match self.filling() {
            Some(b) => {
                self.len[b] = self.pending;
                self.ready += 1;
                self.armed = false;
                true
            }
            None => false,
        }
    }

    /// The oldest unread packet: its buffer and length.
    pub const fn next(&self) -> Option<(usize, u16)> {
        if self.ready != 0 {
            Some((self.head as usize, self.len[self.head as usize]))
        } else {
            None
        }
    }

    /// The firmware read the packet [OutQueue::next] gave.
    pub fn consumed(&mut self) {
        debug_assert!(self.ready != 0);
        self.ready -= 1;
        self.head = (self.head + 1) % self.packets;
    }

    pub const fn ready(&self) -> u8 {
        self.ready
    }

    pub const fn is_armed(&self) -> bool {
        self.armed
    }
}

/// One endpoint number's share: a waker per direction and the OUT queue.
pub struct EpState {
    pub(crate) out_waker: WakerCell,
    pub(crate) in_waker: WakerCell,
    pub(crate) out: Mutex<Cell<OutQueue>>,
}

impl EpState {
    pub const fn new() -> Self {
        EpState {
            out_waker: WakerCell::new(),
            in_waker: WakerCell::new(),
            out: Mutex::new(Cell::new(OutQueue::DISABLED)),
        }
    }

    pub(crate) fn out(&self, cs: CriticalSection) -> OutQueue {
        self.out.borrow(cs).get()
    }

    pub(crate) fn set_out(&self, cs: CriticalSection, q: OutQueue) {
        self.out.borrow(cs).set(q)
    }
}

impl Default for EpState {
    fn default() -> Self {
        Self::new()
    }
}

/// EP0's share beyond its [EpState]: the SETUP packet and the completions of its two directions.
pub struct Ep0State {
    /// The last SETUP packet popped from the receive FIFO, as its two words.
    pub(crate) setup: Mutex<Cell<[u32; 2]>>,
    /// IN XFRC since EP0's IN was last armed.
    pub(crate) in_done: Mutex<Cell<bool>>,
    /// OUT XFRC since EP0's OUT was last armed, with the packet's length.
    pub(crate) out_done: Mutex<Cell<Option<u16>>>,
}

impl Ep0State {
    pub const fn new() -> Self {
        Ep0State {
            setup: Mutex::new(Cell::new([0; 2])),
            in_done: Mutex::new(Cell::new(false)),
            out_done: Mutex::new(Cell::new(None)),
        }
    }
}

impl Default for Ep0State {
    fn default() -> Self {
        Self::new()
    }
}

/// The shared state of a device with `N` endpoint numbers in use and `W` words of OUT buffers
/// ([super::Layout]'s `count` and `out_words`). Declare it as a `static` with [crate::otg_state!].
pub struct State<const N: usize, const W: usize> {
    pub(crate) bus: BusState,
    pub(crate) ep0: Ep0State,
    pub(crate) speed: Mutex<Cell<Speed>>,
    pub(crate) eps: [EpState; N],
    buffers: UnsafeCell<[u32; W]>,
}

// Safety: the buffers are written by the interrupt handler only into the buffer an endpoint's [OutQueue] says is
// being filled, and read by the endpoint's task only from buffers it says are ready; the queue changes in a
// critical section.
unsafe impl<const N: usize, const W: usize> Sync for State<N, W> {}

impl<const N: usize, const W: usize> State<N, W> {
    pub const fn new() -> Self {
        State {
            bus: BusState::new(),
            ep0: Ep0State::new(),
            speed: Mutex::new(Cell::new(Speed::Full)),
            eps: [const { EpState::new() }; N],
            buffers: UnsafeCell::new([0; W]),
        }
    }

    /// The bus status `Usb::run()` keeps, for any task.
    pub fn status(&self) -> crate::v4::Status {
        self.bus.status()
    }

    /// Waits until the bus status differs from `seen` (one waiting task at a time).
    pub async fn status_changed(&self, seen: crate::v4::Status) -> crate::v4::Status {
        self.bus.status_changed(seen).await
    }

    pub(crate) fn as_ref(&'static self) -> StateRef {
        StateRef {
            bus: &self.bus,
            ep0: &self.ep0,
            speed: &self.speed,
            eps: &self.eps,
            buffers: self.buffers.get() as *mut u32,
            words: W,
        }
    }
}

impl<const N: usize, const W: usize> Default for State<N, W> {
    fn default() -> Self {
        Self::new()
    }
}

/// A [State] with its sizes erased, what the interrupt handler finds through the registration.
#[derive(Copy, Clone)]
pub struct StateRef {
    pub(crate) bus: &'static BusState,
    pub(crate) ep0: &'static Ep0State,
    pub(crate) speed: &'static Mutex<Cell<Speed>>,
    pub(crate) eps: &'static [EpState],
    buffers: *mut u32,
    words: usize,
}

// Safety: see State's Sync
unsafe impl Send for StateRef {}
unsafe impl Sync for StateRef {}

impl StateRef {
    pub(crate) fn speed(&self) -> Speed {
        critical_section::with(|cs| self.speed.borrow(cs).get())
    }

    /// Word `i` of the OUT buffers.
    ///
    /// # Safety
    /// The caller owns that word now: by its endpoint's [OutQueue] it is in the buffer being filled (the interrupt
    /// handler) or in a ready one (the endpoint's task).
    pub(crate) unsafe fn set_word(&self, i: usize, value: u32) {
        assert!(i < self.words);
        unsafe { self.buffers.add(i).write_volatile(value) }
    }

    /// # Safety
    /// As for [StateRef::set_word].
    pub(crate) unsafe fn word(&self, i: usize) -> u32 {
        assert!(i < self.words);
        unsafe { self.buffers.add(i).read_volatile() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_queue_arms_while_a_buffer_is_free() {
        let mut q = OutQueue::new(2);
        assert_eq!(q.to_arm(), Some(0));
        q.arm();
        q.copied(10);
        assert!(q.completed());
        // buffer 1 can take the next packet while 0 waits to be read
        assert_eq!(q.to_arm(), Some(1));
        q.arm();
        q.copied(20);
        assert!(q.completed());
        assert_eq!(q.to_arm(), None, "both full");
        assert_eq!(q.next(), Some((0, 10)));
        q.consumed();
        assert_eq!(q.to_arm(), Some(0));
        assert_eq!(q.next(), Some((1, 20)));
        q.consumed();
        assert_eq!(q.next(), None);
    }

    #[test]
    fn single_queue_waits_for_the_read() {
        let mut q = OutQueue::new(1);
        q.arm();
        q.copied(3);
        assert!(q.completed());
        assert_eq!(q.to_arm(), None);
        assert_eq!(q.next(), Some((0, 3)));
        q.consumed();
        assert_eq!(q.to_arm(), Some(0));
    }

    #[test]
    fn disabled_drops() {
        let mut q = OutQueue::DISABLED;
        assert_eq!(q.to_arm(), None);
        assert_eq!(q.filling(), None);
        assert!(!q.completed());
    }
}
