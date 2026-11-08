//! What the interrupt handler and the tasks share: wakers, bus events and the buffer queues of double-buffered
//! endpoints. One [State] per device, sized by its layout ([crate::state!]).
//!
//! thumbv6m has no compare-and-swap, so everything here is behind a `critical_section::Mutex`; the sections are
//! a few loads and stores.

use core::cell::{Cell, RefCell};
use core::task::Waker;

use critical_section::{CriticalSection, Mutex};

/// One task's waker.
pub struct WakerCell(Mutex<RefCell<Option<Waker>>>);

impl WakerCell {
    pub const fn new() -> Self {
        WakerCell(Mutex::new(RefCell::new(None)))
    }

    pub fn register(&self, waker: &Waker) {
        critical_section::with(|cs| {
            let mut w = self.0.borrow_ref_mut(cs);
            match &*w {
                Some(old) if old.will_wake(waker) => {}
                _ => *w = Some(waker.clone()),
            }
        })
    }

    pub fn wake(&self) {
        if let Some(w) = critical_section::with(|cs| self.0.borrow_ref_mut(cs).take()) {
            w.wake()
        }
    }
}

impl Default for WakerCell {
    fn default() -> Self {
        Self::new()
    }
}

/// The IN queue of a double-buffered endpoint: packets the firmware filled, at most two.
///
/// Only one buffer is handed to the peripheral (SW_BUF toggled) at a time; the interrupt handler hands over the
/// second when the first is sent. Handing over both before either is sent can leave DTOG equal to SW_BUF, which
/// the peripheral reads as a buffer conflict and NAKs: the race embassy-stm32 found and fixed this way
/// (embassy-rs/embassy#7275, `usb/double_buffer.rs`). It also keeps one CTR per packet the firmware knows of.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TxQueue {
    /// Packets filled and not yet sent: 0, 1 or 2.
    queued: u8,
    /// The buffer the next packet goes to (0: `TXRXBD`, 1: `RXTXBD`).
    next: u8,
}

impl TxQueue {
    pub const fn new() -> Self {
        TxQueue { queued: 0, next: 0 }
    }

    /// The buffer the firmware may fill now.
    pub const fn free(&self) -> Option<usize> {
        if self.queued < 2 {
            Some(self.next as usize)
        } else {
            None
        }
    }

    /// The buffer [TxQueue::free] gave is filled. True: hand it to the peripheral now (it is the only one queued).
    pub fn push(&mut self) -> bool {
        debug_assert!(self.queued < 2);
        self.queued += 1;
        self.next ^= 1;
        self.queued == 1
    }

    /// A packet was sent (CTR). True: the other buffer is filled, hand it over.
    pub fn sent(&mut self) -> bool {
        self.queued = self.queued.saturating_sub(1);
        self.queued != 0
    }

    pub const fn queued(&self) -> u8 {
        self.queued
    }
}

impl Default for TxQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// The OUT queue of a double-buffered endpoint: packets received and not yet read, at most two.
///
/// As for IN, one buffer at a time is handed to the peripheral; the interrupt handler hands over the other when a
/// packet arrives and it is free, so the peripheral can receive while the firmware reads.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RxQueue {
    /// Packets received and not read: 0, 1 or 2.
    ready: u8,
    /// The buffer of the oldest unread packet.
    head: u8,
    /// A buffer is handed to the peripheral.
    armed: bool,
}

impl RxQueue {
    /// Buffer 0 handed to the peripheral, nothing received.
    pub const fn new() -> Self {
        RxQueue {
            ready: 0,
            head: 0,
            armed: true,
        }
    }

    /// The buffer holding the oldest unread packet.
    pub const fn next(&self) -> Option<usize> {
        if self.ready != 0 {
            Some(self.head as usize)
        } else {
            None
        }
    }

    fn arm_if_free(&mut self) -> bool {
        if !self.armed && self.ready < 2 {
            self.armed = true;
            true
        } else {
            false
        }
    }

    /// A packet arrived in the armed buffer (CTR). True: hand the other buffer to the peripheral.
    pub fn received(&mut self) -> bool {
        if !self.armed {
            return false;
        }
        self.armed = false;
        self.ready += 1;
        self.arm_if_free()
    }

    /// The firmware read the packet [RxQueue::next] gave. True: hand a buffer to the peripheral.
    pub fn consumed(&mut self) -> bool {
        debug_assert!(self.ready != 0);
        self.ready -= 1;
        self.head ^= 1;
        self.arm_if_free()
    }

    pub const fn ready(&self) -> u8 {
        self.ready
    }
}

impl Default for RxQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// The queue of a double-buffered endpoint register, by its direction.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Queue {
    None,
    In(TxQueue),
    Out(RxQueue),
}

/// One endpoint register's share: a waker per direction and, when double-buffered, its queue.
pub struct EpState {
    pub(crate) out_waker: WakerCell,
    pub(crate) in_waker: WakerCell,
    pub(crate) queue: Mutex<Cell<Queue>>,
}

impl EpState {
    pub const fn new() -> Self {
        EpState {
            out_waker: WakerCell::new(),
            in_waker: WakerCell::new(),
            queue: Mutex::new(Cell::new(Queue::None)),
        }
    }

    pub(crate) fn queue(&self, cs: CriticalSection) -> Queue {
        self.queue.borrow(cs).get()
    }

    pub(crate) fn set_queue(&self, cs: CriticalSection, q: Queue) {
        self.queue.borrow(cs).set(q)
    }

    pub(crate) fn wake_both(&self) {
        self.out_waker.wake();
        self.in_waker.wake();
    }
}

impl Default for EpState {
    fn default() -> Self {
        Self::new()
    }
}

/// Bus events the interrupt handler saw and `Usb::run()` hasn't taken yet, as bits.
pub mod event {
    /// RST_DCON: a bus reset; EP0 and the buffer table are set up again already.
    pub const RESET: u8 = 1 << 0;
    /// SUSP: three missed SOFs; SUSPEN is set.
    pub const SUSPEND: u8 = 1 << 1;
    /// WKUP: bus activity while suspended; SUSPEN and SUSPRDY are cleared.
    pub const RESUME: u8 = 1 << 2;
    /// ESOF: an expected SOF missed (only when the firmware unmasks ESOFM).
    pub const ESOF: u8 = 1 << 3;
    /// A SETUP packet is in EP0's receive buffer.
    pub const SETUP: u8 = 1 << 4;
}

/// Device-wide share: bus events and the waker of the task that handles them and EP0.
pub struct BusState {
    pub(crate) waker: WakerCell,
    pub(crate) events: Mutex<Cell<u8>>,
    pub(crate) resets: Mutex<Cell<u8>>,
}

impl BusState {
    pub const fn new() -> Self {
        BusState {
            waker: WakerCell::new(),
            events: Mutex::new(Cell::new(0)),
            resets: Mutex::new(Cell::new(0)),
        }
    }

    /// A bus reset: [event::RESET] raised, a SETUP from before dropped, the reset count moved on.
    pub(crate) fn reset(&self, cs: CriticalSection) {
        let e = self.events.borrow(cs);
        e.set((e.get() & !event::SETUP) | event::RESET);
        let g = self.resets.borrow(cs);
        g.set(g.get().wrapping_add(1));
    }

    /// Bus resets so far (wrapping): an EP0 transfer that started before one sees it ended.
    pub fn resets(&self) -> u8 {
        critical_section::with(|cs| self.resets.borrow(cs).get())
    }

    pub(crate) fn raise(&self, cs: CriticalSection, bits: u8) {
        let e = self.events.borrow(cs);
        e.set(e.get() | bits);
    }

    /// Takes the events in `mask`: those of them that were raised, now cleared.
    pub fn take(&self, mask: u8) -> u8 {
        critical_section::with(|cs| {
            let e = self.events.borrow(cs);
            let v = e.get();
            e.set(v & !mask);
            v & mask
        })
    }

    /// The events raised, left in place.
    pub fn peek(&self) -> u8 {
        critical_section::with(|cs| self.events.borrow(cs).get())
    }
}

impl Default for BusState {
    fn default() -> Self {
        Self::new()
    }
}

/// The shared state of a device with `N` endpoint registers in use (`Layout::count`). Declare it as a `static`
/// with [crate::state!], which takes `N` from the descriptors.
pub struct State<const N: usize> {
    pub(crate) bus: BusState,
    pub(crate) eps: [EpState; N],
}

impl<const N: usize> State<N> {
    pub const fn new() -> Self {
        State {
            bus: BusState::new(),
            eps: [const { EpState::new() }; N],
        }
    }

    pub(crate) fn as_ref(&'static self) -> StateRef {
        StateRef {
            bus: &self.bus,
            eps: &self.eps,
        }
    }
}

impl<const N: usize> Default for State<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// A [State] with its size erased, what the interrupt handler finds through the registration.
#[derive(Copy, Clone)]
pub struct StateRef {
    pub(crate) bus: &'static BusState,
    pub(crate) eps: &'static [EpState],
}

#[cfg(test)]
mod tests {
    use super::*;

    // The peripheral's selectors for an IN endpoint (Table 228): it sends from buffer DTOG while DTOG != SW_BUF,
    // and toggles DTOG after each packet.
    #[test]
    fn tx_queue_follows_the_selectors() {
        let mut q = TxQueue::new();
        let (mut dtog, mut sw_buf) = (0usize, 0usize);
        let mut bufs = [None; 2];
        let mut sent = 0;
        for packet in 0..50 {
            let b = q.free().unwrap();
            assert!(bufs[b].is_none(), "never overwrite a queued packet");
            bufs[b] = Some(packet);
            if q.push() {
                sw_buf ^= 1;
            }
            if q.free().is_none() {
                assert_ne!(dtog, sw_buf, "the peripheral owns a filled buffer");
                assert_eq!(bufs[dtog].take(), Some(sent));
                sent += 1;
                dtog ^= 1;
                assert_eq!(
                    dtog, sw_buf,
                    "conflict: NAK until the handler hands over the next"
                );
                assert!(q.sent());
                sw_buf ^= 1;
            }
        }
        assert_eq!(bufs[dtog].take(), Some(sent));
        assert!(!q.sent());
        assert_eq!(q.queued(), 0);
    }

    // OUT: the peripheral writes buffer DTOG while DTOG != SW_BUF.
    #[test]
    fn rx_queue_never_hands_over_an_unread_buffer() {
        let mut q = RxQueue::new();
        let (mut dtog, mut sw_buf) = (0usize, 1usize);
        let mut bufs = [None; 2];
        let mut read = 0;
        for packet in 0..50 {
            assert_ne!(dtog, sw_buf);
            assert!(bufs[dtog].is_none(), "the peripheral writes a free buffer");
            bufs[dtog] = Some(packet);
            dtog ^= 1;
            if q.received() {
                sw_buf ^= 1;
            } else {
                let h = q.next().unwrap();
                assert_eq!(bufs[h].take(), Some(read));
                read += 1;
                assert!(q.consumed());
                sw_buf ^= 1;
            }
        }
        while let Some(h) = q.next() {
            assert_eq!(bufs[h].take(), Some(read));
            read += 1;
            q.consumed();
        }
        assert_eq!(bufs, [None; 2]);
    }

    #[test]
    fn rx_queue_full() {
        let mut q = RxQueue::new();
        assert!(q.received()); // buffer 1 handed over while 0 waits
        assert!(!q.received()); // both full
        assert!(!q.received()); // no third packet can arrive
        assert_eq!(q.next(), Some(0));
        assert!(q.consumed()); // buffer 0 free again
        assert_eq!(q.next(), Some(1));
        assert!(!q.consumed()); // already armed
        assert_eq!(q.next(), None);
    }

    #[test]
    fn events_take() {
        let b = BusState::new();
        critical_section::with(|cs| b.raise(cs, event::RESET | event::SETUP));
        assert_eq!(b.take(event::SETUP), event::SETUP);
        assert_eq!(b.peek(), event::RESET);
        assert_eq!(b.take(0xff), event::RESET);
        assert_eq!(b.peek(), 0);
    }
}
