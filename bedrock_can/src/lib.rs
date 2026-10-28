//! An FDCAN (Bosch M_CAN) channel as `ww_can_bus::Can`, the part without a HAL: bit timing from a rate and sample
//! point, checks of a host's configuration against what the controller and the message RAM layout of the firmware
//! can do ([Controller]), mapping of acceptance filters onto M_CAN filter elements, frames to send, error state.
//!
//! The firmware (the template's `src/can.rs`) turns the results into calls of the `mcan` driver; a host tool uses the
//! same functions to compute what a device will answer. Everything here answers with a `CanError` instead of
//! panicking: every value comes from the host.
//!
//! Ported from the B135 firmware (`b135_board::can`), with the kernel clock and the layout as parameters.

#![no_std]

use wire_weaver::prelude::*;
use ww_can_bus::{
    Activity, BitTiming, CanConfig, CanError, CanId, Capabilities, ErrorState, FdConfig, Filter,
    FilterAction, FilterConfig, FilterMatch, Frame, FrameKind, Mode, NonMatching, Ohm, Options,
    ProtocolError, RxOverflow, Second, Status, Tdc, TimestampInfo, TimestampPoint, Timing,
    TimingLimits, TransceiverModes, TxOrder,
};
use ww_si::Prefix;
use ww_si::ww_numeric::NumericValue;

/// Nominal bit timing as M_CAN `NBTP` holds it (RM0468 FDCAN_NBTP, `mcan::NominalBitTiming`).
pub const NOMINAL: TimingLimits = TimingLimits {
    prescaler_min: 1,
    prescaler_max: 512,
    seg1_min: 2,
    seg1_max: 256,
    seg2_min: 1,
    seg2_max: 128,
    sjw_max: 128,
};

/// Data bit timing as M_CAN `DBTP` holds it (RM0468 FDCAN_DBTP, `mcan::DataBitTiming`).
pub const DATA: TimingLimits = TimingLimits {
    prescaler_min: 1,
    prescaler_max: 32,
    seg1_min: 1,
    seg1_max: 32,
    seg2_min: 1,
    seg2_max: 16,
    sjw_max: 16,
};

/// Most standard filter elements a [Controller] may have ([HwFilters] holds this many).
pub const STANDARD_FILTERS: usize = 64;
/// Most extended filter elements a [Controller] may have.
pub const EXTENDED_FILTERS: usize = 32;
/// Transceiver delay compensation is possible with a data prescaler of 1 or 2 only (RM0468, FDCAN_DBTP.TDC).
pub const TDC_MAX_PRESCALER: u16 = 2;
/// Largest TDC offset and filter window (`TDCR.TDCO` / `TDCF`, 7 bits), in clock periods.
pub const TDC_MAX: u8 = 127;

/// One FDCAN instance as the firmware runs it: its kernel clock and the message RAM layout the driver gives it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Controller {
    /// FDCAN kernel clock (`fdcan_ker_ck`), the time quantum's source.
    pub clock_hz: u32,
    /// Standard filter elements, at most [STANDARD_FILTERS].
    pub standard_filters: u8,
    /// Extended filter elements, at most [EXTENDED_FILTERS].
    pub extended_filters: u8,
    /// Elements of receive FIFO 0, where every frame goes unless a filter says FIFO 1.
    pub rx_fifo0: u8,
    /// Elements of receive FIFO 1.
    pub rx_fifo1: u8,
    /// Elements of the transmit FIFO.
    pub tx_fifo: u8,
}

impl Controller {
    /// An STM32H7 FDCAN with the layout the template's firmware builds (as B135 does): 64 standard and 32 extended
    /// filters, receive FIFOs of 64 and 32 elements, a transmit FIFO of 32, all elements with 64 data bytes; 2432 of
    /// the 2560 words of message RAM, so one instance uses most of it. A second instance gets [Controller::h7_half].
    pub const fn h7(clock_hz: u32) -> Controller {
        Controller {
            clock_hz,
            standard_filters: 64,
            extended_filters: 32,
            rx_fifo0: 64,
            rx_fifo1: 32,
            tx_fifo: 32,
        }
    }

    /// An STM32H7 FDCAN sharing the message RAM with others: 32 standard and 16 extended filters, receive FIFOs of 24
    /// and 8 elements, a transmit FIFO of 16 (928 words with 64-byte elements, so two fit beside each other in the
    /// 2560 words; [Controller::h7] leaves room for no other).
    pub const fn h7_half(clock_hz: u32) -> Controller {
        Controller {
            clock_hz,
            standard_filters: 32,
            extended_filters: 16,
            rx_fifo0: 24,
            rx_fifo1: 8,
            tx_fifo: 16,
        }
    }

    /// An STM32G0 (or G4) FDCAN: its message RAM is fixed, receive FIFOs of 3 elements and a transmit FIFO of 3.
    /// The `mcan` driver has no filter elements on these parts yet, so only the global filter (non-matching frames
    /// into FIFO 0 / 1 or rejected, remote frames rejected) is there.
    pub const fn g0(clock_hz: u32) -> Controller {
        Controller {
            clock_hz,
            standard_filters: 0,
            extended_filters: 0,
            rx_fifo0: 3,
            rx_fifo1: 3,
            tx_fifo: 3,
        }
    }

    /// Filters a host may set in all: every element of both kinds.
    pub const fn max_filters(&self) -> usize {
        self.standard_filters as usize + self.extended_filters as usize
    }
}

/// What the channel supports. `rx_fifo_depth` and `tx_queue_depth` are the firmware's software queues in front of
/// the controller's FIFOs; `timestamps` are taken when the firmware reads a frame out (the controller's own
/// timestamp counter runs in bit times and changes its clock with the bit rate switch). `termination`: the
/// terminator values the board can switch in, empty without one.
pub fn capabilities(
    controller: &Controller,
    rx_queue: u16,
    tx_queue: u16,
    termination: &'static [Ohm],
) -> Capabilities<'static> {
    Capabilities {
        clock: ww_can_bus::Hertz::from_parts(Prefix::Unit, NumericValue::U32(controller.clock_hz)),
        nominal: NOMINAL,
        data: Some(DATA),
        tdc: true,
        restricted: true,
        listen_only: true,
        internal_loopback: true,
        external_loopback: true,
        test_mode: false,
        sleep: false,
        one_shot: true,
        presume_ack: false,
        triple_sample: false,
        tx_pause: true,
        tx_order: false,
        rx_overflow: false,
        edge_filtering: true,
        protocol_exception: true,
        wake_on_bus: false,
        long_dlc: false,
        auto_recovery: true,
        timestamps: Some(TimestampInfo {
            resolution: Second::from_parts(Prefix::Micro, NumericValue::U8(1)),
            point: TimestampPoint::Software,
        }),
        rx_fifos: 2,
        rx_fifo_depth: rx_queue,
        tx_queue_depth: tx_queue,
        tx_buffers: 0,
        rx_buffers: 0,
        standard_filters: controller.standard_filters as u16,
        extended_filters: controller.extended_filters as u16,
        filter_mask: controller.max_filters() > 0,
        filter_range: controller.max_filters() > 0,
        memory: None,
        transceiver_modes: TransceiverModes {
            standby: false,
            silent: false,
            sleep: false,
        },
        termination: MaxLen::new_unchecked(RefVec::Slice { slice: termination }),
        performance: None,
    }
}

fn invalid(arg: u8, reason: &'static str) -> CanError<'static> {
    CanError::Common(CommonError::InvalidArgument {
        arg,
        reason: MaxLen::new_unchecked(reason),
    })
}

fn unsupported() -> CanError<'static> {
    CanError::Common(CommonError::Unsupported)
}

/// `bt` is within `limits` and a valid bit (`sjw` no longer than either phase segment).
pub fn check_segments(bt: &BitTiming, limits: &TimingLimits) -> Result<(), CanError<'static>> {
    let ok = (limits.prescaler_min..=limits.prescaler_max).contains(&bt.prescaler)
        && (limits.seg1_min..=limits.seg1_max).contains(&bt.seg1)
        && (limits.seg2_min..=limits.seg2_max).contains(&bt.seg2)
        && bt.sjw >= 1
        && bt.sjw <= limits.sjw_max
        && bt.sjw <= bt.seg2
        && bt.sjw as u16 <= bt.seg1;
    if ok {
        Ok(())
    } else {
        Err(invalid(0, "bit timing outside the controller's limits"))
    }
}

/// Segments for `rate` bit/s sampled at `sample_point` tenths of a percent, at an exact bit rate: the prescaler with
/// the sample point closest to the one asked for, the smallest such prescaler (most quanta) on a tie. `sjw` is as
/// long as the shorter phase segment allows. `None` when no prescaler divides the clock into a whole number of quanta
/// per bit that the limits allow.
pub fn rate_segments(
    clock_hz: u32,
    rate: u32,
    sample_point: u16,
    limits: &TimingLimits,
) -> Option<BitTiming> {
    if rate == 0 || sample_point == 0 || sample_point >= 1000 {
        return None;
    }
    let min_quanta = 1 + limits.seg1_min as u32 + limits.seg2_min as u32;
    let max_quanta = 1 + limits.seg1_max as u32 + limits.seg2_max as u32;
    let mut best: Option<(u32, BitTiming)> = None;
    for prescaler in limits.prescaler_min..=limits.prescaler_max {
        let per_bit = prescaler as u64 * rate as u64;
        if !(clock_hz as u64).is_multiple_of(per_bit) {
            continue;
        }
        let quanta = (clock_hz as u64 / per_bit) as u32;
        if quanta < min_quanta || quanta > max_quanta {
            continue;
        }
        // quanta up to the sample point, rounded, then moved into the limits
        let before = (quanta * sample_point as u32 + 500) / 1000;
        let seg2 = (quanta - before).clamp(limits.seg2_min as u32, limits.seg2_max as u32);
        let seg1 = quanta - 1 - seg2;
        if seg1 < limits.seg1_min as u32 || seg1 > limits.seg1_max as u32 {
            continue;
        }
        let sjw = seg2.min(seg1).min(limits.sjw_max as u32);
        let bt = BitTiming {
            prescaler,
            seg1: seg1 as u16,
            seg2: seg2 as u8,
            sjw: sjw as u8,
        };
        let error = (bt.sample_point() as i32 - sample_point as i32).unsigned_abs();
        if best.as_ref().is_none_or(|(e, _)| error < *e) {
            best = Some((error, bt));
        }
    }
    best.map(|(_, bt)| bt)
}

/// Segments of `timing`: checked when given as segments, computed when given as a rate.
pub fn segments(
    clock_hz: u32,
    timing: &Timing,
    limits: &TimingLimits,
) -> Result<BitTiming, CanError<'static>> {
    match timing {
        Timing::Segments(bt) => {
            check_segments(bt, limits)?;
            Ok(*bt)
        }
        Timing::Rate { rate, sample_point } => {
            let rate = rate
                .to_u32_in(Prefix::Unit)
                .ok_or(invalid(0, "bit rate out of range"))?;
            rate_segments(clock_hz, rate, *sample_point, limits).ok_or(invalid(
                0,
                "bit rate not reachable exactly at the controller clock",
            ))
        }
        _ => Err(unsupported()),
    }
}

/// What happens after bus-off.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Recovery {
    /// Stays bus-off until `recover`.
    Manual,
    /// Recovers at once (the driver's interrupt handler leaves INIT).
    Immediate,
    /// Recovers this many milliseconds after entering bus-off.
    AfterMs(u32),
}

/// The data phase as the controller is set up for it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DataPhase {
    pub timing: BitTiming,
    /// TDC on, with offset and filter window in clock periods.
    pub tdc: Option<(u8, u8)>,
    pub iso: bool,
}

/// A checked configuration, in the terms of the controller.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct HwConfig {
    pub mode: Mode,
    pub nominal: BitTiming,
    /// `None`: classic CAN only.
    pub data: Option<DataPhase>,
    pub one_shot: bool,
    pub tx_pause: bool,
    pub edge_filtering: bool,
    pub protocol_exception: bool,
    pub recovery: Recovery,
}

/// 250 kbit/s classic at 87.5 %, normal mode, automatic recovery after 100 ms: what a channel starts with. `None`
/// when the kernel clock can't make 250 kbit/s exactly (it can from any whole number of MHz the limits allow).
pub fn default_config(controller: &Controller) -> Option<HwConfig> {
    Some(HwConfig {
        mode: Mode::Normal,
        nominal: rate_segments(controller.clock_hz, 250_000, 875, &NOMINAL)?,
        data: None,
        one_shot: false,
        tx_pause: false,
        edge_filtering: false,
        protocol_exception: true,
        recovery: Recovery::AfterMs(100),
    })
}

/// `mode` is one this channel has.
pub fn check_mode(mode: Mode) -> Result<Mode, CanError<'static>> {
    match mode {
        Mode::Off
        | Mode::Config
        | Mode::Normal
        | Mode::Restricted
        | Mode::ListenOnly
        | Mode::InternalLoopback
        | Mode::ExternalLoopback => Ok(mode),
        _ => Err(unsupported()),
    }
}

/// The controller is on the bus in `mode` (its error state means something).
pub fn mode_on_bus(mode: Mode) -> bool {
    !matches!(mode, Mode::Off | Mode::Config)
}

/// Frames queued in `mode` go out (on the bus, or back in a loopback).
pub fn mode_sends(mode: Mode) -> bool {
    matches!(
        mode,
        Mode::Normal | Mode::InternalLoopback | Mode::ExternalLoopback
    )
}

fn check_options(o: &Options) -> Result<(), CanError<'static>> {
    let missing = o.presume_ack
        || o.triple_sample
        || o.tx_order != TxOrder::Fifo
        || o.rx_overflow != RxOverflow::KeepOldest
        || o.wake_on_bus;
    if missing { Err(unsupported()) } else { Ok(()) }
}

/// Checks a host's configuration and computes the segments; the answer of `configure` is
/// [HwConfig::to_config] of the result.
pub fn resolve(config: &CanConfig, controller: &Controller) -> Result<HwConfig, CanError<'static>> {
    let clock_hz = controller.clock_hz;
    let mode = check_mode(config.mode)?;
    let nominal = segments(clock_hz, &config.nominal, &NOMINAL)?;
    check_options(&config.options)?;
    let data = match &config.fd {
        None => None,
        Some(fd) => Some(resolve_fd(fd, clock_hz)?),
    };
    let recovery = match &config.bus_off_recovery {
        None => Recovery::Manual,
        Some(t) => match t.to_u32_in(Prefix::Milli) {
            Some(0) => Recovery::Immediate,
            Some(ms) => Recovery::AfterMs(ms),
            None => return Err(invalid(0, "bus-off recovery time out of range")),
        },
    };
    Ok(HwConfig {
        mode,
        nominal,
        data,
        one_shot: config.options.one_shot,
        tx_pause: config.options.tx_pause,
        edge_filtering: config.options.edge_filtering,
        protocol_exception: config.options.protocol_exception,
        recovery,
    })
}

fn resolve_fd(fd: &FdConfig, clock_hz: u32) -> Result<DataPhase, CanError<'static>> {
    let timing = segments(clock_hz, &fd.data, &DATA)?;
    let tdc = match fd.tdc {
        Tdc::Off => None,
        Tdc::Auto => {
            // secondary sample point at the data sample point, in clock periods
            let offset = (1 + timing.seg1 as u32) * timing.prescaler as u32;
            Some((offset, 0))
        }
        Tdc::Manual { offset, filter } => Some((offset as u32, filter.unwrap_or(0) as u32)),
        _ => return Err(unsupported()),
    };
    if let Some((offset, filter)) = tdc {
        if timing.prescaler > TDC_MAX_PRESCALER {
            return Err(invalid(0, "TDC needs a data prescaler of 1 or 2"));
        }
        if offset > TDC_MAX as u32 || filter > TDC_MAX as u32 {
            return Err(invalid(0, "TDC offset or filter above 127 clock periods"));
        }
    }
    Ok(DataPhase {
        timing,
        tdc: tdc.map(|(o, f)| (o as u8, f as u8)),
        iso: fd.iso,
    })
}

impl HwConfig {
    /// The configuration as `configure` answers it: segments, TDC as set.
    pub fn to_config(&self) -> CanConfig {
        CanConfig {
            mode: self.mode,
            nominal: Timing::Segments(self.nominal),
            fd: self.data.map(|d| FdConfig {
                data: Timing::Segments(d.timing),
                tdc: match d.tdc {
                    None => Tdc::Off,
                    Some((offset, filter)) => Tdc::Manual {
                        offset,
                        filter: (filter != 0).then_some(filter),
                    },
                },
                iso: d.iso,
            }),
            options: Options {
                one_shot: self.one_shot,
                presume_ack: false,
                triple_sample: false,
                tx_pause: self.tx_pause,
                tx_order: TxOrder::Fifo,
                rx_overflow: RxOverflow::KeepOldest,
                edge_filtering: self.edge_filtering,
                protocol_exception: self.protocol_exception,
                wake_on_bus: false,
            },
            bus_off_recovery: match self.recovery {
                Recovery::Manual => None,
                Recovery::Immediate => Some(Second::from_parts(Prefix::Milli, NumericValue::U8(0))),
                Recovery::AfterMs(ms) => {
                    Some(Second::from_parts(Prefix::Milli, NumericValue::U32(ms)))
                }
            },
        }
    }
}

// ---- filters ----

/// Which IDs an M_CAN filter element matches (`SFT` / `EFT`), raw IDs.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum HwMatch {
    Range {
        from: u32,
        to: u32,
    },
    /// Dual ID: one of the two (both the same for one ID).
    Dual(u32, u32),
    Mask {
        id: u32,
        mask: u32,
    },
}

/// What an M_CAN filter element does with a match (`SFEC` / `EFEC`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum HwAction {
    Fifo { fifo: u8, priority: bool },
    Reject,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct HwFilter {
    pub matching: HwMatch,
    pub action: HwAction,
}

/// What happens to frames no element matches: into FIFO 0 / 1 or rejected (`ANFS` / `ANFE`).
pub type HwNonMatching = Option<u8>;

/// An acceptance filter list as the controller holds it, and where each element is in the host's list, for telling
/// which filter accepted a received frame (the controller reports the element index `FIDX` within the standard or
/// the extended list).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct HwFilters {
    pub standard: [Option<HwFilter>; STANDARD_FILTERS],
    pub extended: [Option<HwFilter>; EXTENDED_FILTERS],
    pub non_matching_standard: HwNonMatching,
    pub non_matching_extended: HwNonMatching,
    pub reject_remote: bool,
    standard_host: [u8; STANDARD_FILTERS],
    extended_host: [u8; EXTENDED_FILTERS],
}

impl HwFilters {
    /// No filters: every frame into FIFO 0, as without `set_filters`.
    pub const fn accept_all() -> HwFilters {
        HwFilters {
            standard: [None; STANDARD_FILTERS],
            extended: [None; EXTENDED_FILTERS],
            non_matching_standard: Some(0),
            non_matching_extended: Some(0),
            reject_remote: false,
            standard_host: [0; STANDARD_FILTERS],
            extended_host: [0; EXTENDED_FILTERS],
        }
    }

    /// Maps a host's filter list onto the elements: first match wins on both sides, since standard and extended
    /// lists keep the host's order and never match each other's IDs.
    pub fn map(
        config: &FilterConfig<'_>,
        controller: &Controller,
    ) -> Result<HwFilters, CanError<'static>> {
        let max = controller.max_filters();
        let standard_max = (controller.standard_filters as usize).min(STANDARD_FILTERS);
        let extended_max = (controller.extended_filters as usize).min(EXTENDED_FILTERS);
        let mut hw = HwFilters::accept_all();
        let (mut std, mut ext, mut n) = (0, 0, 0);
        for filter in config.filters.iter() {
            let filter = filter.map_err(|_| invalid(0, "malformed filter"))?;
            if n >= max {
                return Err(CanError::OutOfFilters { max: max as u16 });
            }
            let (extended, element) = map_filter(&filter)?;
            if extended {
                if ext >= extended_max {
                    return Err(CanError::OutOfFilters {
                        max: extended_max as u16,
                    });
                }
                hw.extended[ext] = Some(element);
                hw.extended_host[ext] = n as u8;
                ext += 1;
            } else {
                if std >= standard_max {
                    return Err(CanError::OutOfFilters {
                        max: standard_max as u16,
                    });
                }
                hw.standard[std] = Some(element);
                hw.standard_host[std] = n as u8;
                std += 1;
            }
            n += 1;
        }
        hw.non_matching_standard = non_matching(config.non_matching_standard)?;
        hw.non_matching_extended = non_matching(config.non_matching_extended)?;
        hw.reject_remote = config.reject_remote;
        Ok(hw)
    }

    /// Index into the host's list of the filter behind element `element` of the standard or extended list (the
    /// `FIDX` of a received frame), `None` for an element the host didn't set.
    pub fn host_index(&self, extended: bool, element: u8) -> Option<u32> {
        let i = element as usize;
        let (set, host) = if extended {
            (self.extended.get(i)?, self.extended_host.get(i)?)
        } else {
            (self.standard.get(i)?, self.standard_host.get(i)?)
        };
        set.map(|_| *host as u32)
    }
}

fn non_matching(n: NonMatching) -> Result<HwNonMatching, CanError<'static>> {
    match n {
        NonMatching::Accept { fifo } if fifo <= 1 => Ok(Some(fifo)),
        NonMatching::Accept { .. } => Err(invalid(0, "receive FIFO 0 or 1 only")),
        NonMatching::Reject => Ok(None),
        _ => Err(unsupported()),
    }
}

/// Whether an ID is extended, and its raw value.
fn raw(id: CanId) -> (bool, u32) {
    (id.is_extended(), id.raw())
}

fn map_filter(f: &Filter) -> Result<(bool, HwFilter), CanError<'static>> {
    let (extended, matching) = match f.matching {
        FilterMatch::Exact(id) => {
            let (ext, id) = raw(id);
            (ext, HwMatch::Dual(id, id))
        }
        FilterMatch::Mask { id, mask } => {
            let (ext, id) = raw(id);
            let mask = mask.value();
            if !ext && mask > 0x7FF {
                return Err(invalid(0, "mask of a standard ID wider than 11 bits"));
            }
            (ext, HwMatch::Mask { id, mask })
        }
        FilterMatch::Range { from, to } => {
            let (ext, from) = raw(from);
            let (ext_to, to) = raw(to);
            if ext != ext_to || from > to {
                return Err(invalid(0, "range of IDs of one kind, from <= to"));
            }
            (ext, HwMatch::Range { from, to })
        }
        _ => return Err(unsupported()),
    };
    let action = match f.action {
        FilterAction::Fifo {
            fifo,
            high_priority,
        } if fifo <= 1 => HwAction::Fifo {
            fifo,
            priority: high_priority,
        },
        FilterAction::Fifo { .. } => return Err(invalid(0, "receive FIFO 0 or 1 only")),
        FilterAction::Reject => HwAction::Reject,
        // no dedicated receive buffers in the driver's message RAM layout
        _ => return Err(unsupported()),
    };
    Ok((extended, HwFilter { matching, action }))
}

// ---- frames ----

/// CAN FD data lengths, by DLC 9..15.
const FD_LENGTHS: [u8; 7] = [12, 16, 20, 24, 32, 48, 64];

/// The FD frame length `len` bytes are sent in (padded with zeroes), `None` above 64.
pub fn fd_len(len: usize) -> Option<u8> {
    if len <= 8 {
        return Some(len as u8);
    }
    FD_LENGTHS.iter().copied().find(|&l| l as usize >= len)
}

/// A frame to hand to the controller.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TxPlan {
    pub extended: bool,
    pub id: u32,
    pub fd: bool,
    pub brs: bool,
    pub rtr: bool,
    /// Length on the bus: the remote frame's DLC, or the data padded up to an FD length.
    pub len: u8,
}

/// Why a frame of `tx` is not sent.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TxReject {
    /// An FD frame while FD is off.
    FdOff,
    /// More data than the frame kind carries, or data in a remote frame.
    BadLength,
    /// A kind or ID this controller doesn't send (long DLC, a newer kind).
    Unsupported,
}

/// Checks a frame of `tx` against the configuration (`fd_on`: FD configured).
pub fn tx_plan(frame: &Frame<'_>, fd_on: bool) -> Result<TxPlan, TxReject> {
    let (extended, id) = raw(frame.id);
    let data_len = frame.data.len();
    let (fd, brs, rtr, len) = match frame.kind {
        FrameKind::Data if data_len <= 8 => (false, false, false, data_len as u8),
        FrameKind::Remote { len } if data_len == 0 && len.value() <= 8 => {
            (false, false, true, len.value())
        }
        FrameKind::Fd { brs, .. } => {
            if !fd_on {
                return Err(TxReject::FdOff);
            }
            (
                true,
                brs,
                false,
                fd_len(data_len).ok_or(TxReject::BadLength)?,
            )
        }
        FrameKind::Data | FrameKind::Remote { .. } => return Err(TxReject::BadLength),
        _ => return Err(TxReject::Unsupported),
    };
    Ok(TxPlan {
        extended,
        id,
        fd,
        brs,
        rtr,
        len,
    })
}

// ---- status ----

/// A last error code (`PSR.LEC` / `DLEC`): `None` for 0 (no error) and 7 (no change since the last read).
pub fn protocol_error(code: u8) -> Option<ProtocolError> {
    match code {
        1 => Some(ProtocolError::Stuff),
        2 => Some(ProtocolError::Form),
        3 => Some(ProtocolError::Ack),
        4 => Some(ProtocolError::Bit1),
        5 => Some(ProtocolError::Bit0),
        6 => Some(ProtocolError::Crc),
        _ => None,
    }
}

/// `PSR.ACT`.
pub fn activity(act: u8) -> Activity {
    match act & 3 {
        0 => Activity::Synchronizing,
        1 => Activity::Idle,
        2 => Activity::Receiving,
        _ => Activity::Transmitting,
    }
}

/// The controller's error state as `PSR` / `ECR` say it.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct HwState {
    pub bus_off: bool,
    pub passive: bool,
    pub warning: bool,
    pub tec: u8,
    pub rec: u8,
}

/// [Status] of the channel in `mode`, with the last errors collected since the last read.
pub fn status(
    mode: Mode,
    hw: HwState,
    last_error: Option<ProtocolError>,
    last_data_error: Option<ProtocolError>,
) -> Status {
    let state = if !mode_on_bus(mode) {
        ErrorState::Stopped
    } else if hw.bus_off {
        ErrorState::BusOff
    } else if hw.passive {
        ErrorState::Passive
    } else if hw.warning {
        ErrorState::Warning
    } else {
        ErrorState::Active
    };
    Status {
        state,
        tec: hw.tec,
        rec: hw.rec,
        last_error,
        last_data_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ww_can_bus::Baud;

    /// B135's kernel clock (PLL2P): 80 MHz hits every common nominal and data rate exactly (2, 4, 5 and 8 Mbit/s
    /// too, which 135 MHz can't).
    const CLOCK_HZ: u32 = 80_000_000;
    const H7: Controller = Controller::h7(CLOCK_HZ);

    fn rate(r: u32, sp: u16) -> Timing {
        Timing::Rate {
            rate: Baud::from_parts(Prefix::Unit, NumericValue::U32(r)),
            sample_point: sp,
        }
    }

    #[test]
    fn common_rates_exact_at_80_mhz() {
        for (r, sp) in [
            (10_000, 875),
            (20_000, 875),
            (50_000, 875),
            (125_000, 875),
            (250_000, 875),
            (500_000, 875),
            (800_000, 800),
            (1_000_000, 800),
        ] {
            let bt = segments(CLOCK_HZ, &rate(r, sp), &NOMINAL).unwrap();
            assert_eq!(bt.bit_rate(CLOCK_HZ), Some(r), "{r}");
            assert!(CLOCK_HZ.is_multiple_of(bt.prescaler as u32 * bt.quanta()));
            assert!(
                (bt.sample_point() as i32 - sp as i32).abs() <= 10,
                "{r}: {bt:?}"
            );
            check_segments(&bt, &NOMINAL).unwrap();
        }
        for r in [1_000_000, 2_000_000, 4_000_000, 5_000_000, 8_000_000] {
            let bt = segments(CLOCK_HZ, &rate(r, 750), &DATA).unwrap();
            assert_eq!(bt.bit_rate(CLOCK_HZ), Some(r), "{r}");
            check_segments(&bt, &DATA).unwrap();
        }
    }

    #[test]
    fn rate_prefers_most_quanta() {
        // 500 kbit/s at 87.5 %: 160 quanta of one clock period
        let bt = rate_segments(CLOCK_HZ, 500_000, 875, &NOMINAL).unwrap();
        assert_eq!(
            bt,
            BitTiming {
                prescaler: 1,
                seg1: 139,
                seg2: 20,
                sjw: 20
            }
        );
        // 2 Mbit/s data: 40 quanta of one clock period
        let bt = rate_segments(CLOCK_HZ, 2_000_000, 750, &DATA).unwrap();
        assert_eq!((bt.prescaler, bt.quanta(), bt.sample_point()), (1, 40, 750));
        // 1 Mbit/s data: 80 quanta don't fit seg1 <= 31 + seg2 <= 15, so prescaler 2 with 40
        let bt = rate_segments(CLOCK_HZ, 1_000_000, 750, &DATA).unwrap();
        assert_eq!((bt.prescaler, bt.quanta(), bt.sample_point()), (2, 40, 750));
    }

    #[test]
    fn unreachable_rates_rejected() {
        // 135 MHz (the old kernel clock) can't make 2 Mbit/s
        assert!(rate_segments(135_000_000, 2_000_000, 750, &DATA).is_none());
        assert!(segments(CLOCK_HZ, &rate(333_333, 875), &NOMINAL).is_err());
        assert!(segments(CLOCK_HZ, &rate(0, 875), &NOMINAL).is_err());
        assert!(segments(CLOCK_HZ, &rate(500_000, 0), &NOMINAL).is_err());
        assert!(segments(CLOCK_HZ, &rate(500_000, 1000), &NOMINAL).is_err());
        // 40 Mbit/s: 2 quanta, fewer than a bit has
        assert!(segments(CLOCK_HZ, &rate(40_000_000, 750), &DATA).is_err());
    }

    #[test]
    fn segments_checked() {
        let ok = BitTiming {
            prescaler: 1,
            seg1: 13,
            seg2: 2,
            sjw: 1,
        };
        check_segments(&ok, &DATA).unwrap();
        for bad in [
            BitTiming { prescaler: 0, ..ok },
            BitTiming {
                prescaler: 33,
                ..ok
            },
            BitTiming { seg1: 33, ..ok },
            BitTiming { seg2: 0, ..ok },
            BitTiming { seg2: 17, ..ok },
            BitTiming { sjw: 0, ..ok },
            BitTiming { sjw: 3, ..ok },
        ] {
            assert!(check_segments(&bad, &DATA).is_err(), "{bad:?}");
        }
        assert!(check_segments(&BitTiming { seg1: 1, ..ok }, &NOMINAL).is_err());
        assert!(
            check_segments(
                &BitTiming {
                    prescaler: 512,
                    ..ok
                },
                &NOMINAL
            )
            .is_ok()
        );
        assert!(
            check_segments(
                &BitTiming {
                    prescaler: 513,
                    ..ok
                },
                &NOMINAL
            )
            .is_err()
        );
        // the largest values the registers hold
        let max = BitTiming {
            prescaler: 32,
            seg1: 32,
            seg2: 16,
            sjw: 16,
        };
        check_segments(&max, &DATA).unwrap();
    }

    fn options() -> Options {
        HwConfig::to_config(&default_config(&H7).unwrap()).options
    }

    fn config(fd: Option<FdConfig>) -> CanConfig {
        CanConfig {
            mode: Mode::Normal,
            nominal: rate(500_000, 875),
            fd,
            options: options(),
            bus_off_recovery: Some(Second::from_parts(Prefix::Milli, NumericValue::U8(100))),
        }
    }

    #[test]
    fn resolve_classic_and_fd() {
        let hw = resolve(&config(None), &H7).unwrap();
        assert_eq!(hw.data, None);
        assert_eq!(hw.recovery, Recovery::AfterMs(100));
        let answer = hw.to_config();
        assert!(matches!(answer.nominal, Timing::Segments(_)));
        // the answer configures the same again
        assert_eq!(resolve(&answer, &H7).unwrap(), hw);

        let fd = FdConfig {
            data: rate(2_000_000, 750),
            tdc: Tdc::Auto,
            iso: true,
        };
        let hw = resolve(&config(Some(fd)), &H7).unwrap();
        let data = hw.data.unwrap();
        // sample point after 15 of 20 quanta of 2 clock periods
        assert_eq!(data.tdc, Some((30, 0)));
        assert_eq!(resolve(&hw.to_config(), &H7).unwrap(), hw);

        // TDC with prescaler 4 (1 Mbit/s data at 20 quanta) is not possible
        let fd = FdConfig {
            data: Timing::Segments(BitTiming {
                prescaler: 4,
                seg1: 14,
                seg2: 5,
                sjw: 5,
            }),
            tdc: Tdc::Auto,
            iso: true,
        };
        assert!(resolve(&config(Some(fd)), &H7).is_err());
        let fd = FdConfig {
            tdc: Tdc::Off,
            ..fd
        };
        assert!(resolve(&config(Some(fd)), &H7).is_ok());
        let fd = FdConfig {
            data: rate(2_000_000, 750),
            tdc: Tdc::Manual {
                offset: 128,
                filter: None,
            },
            iso: true,
        };
        assert!(resolve(&config(Some(fd)), &H7).is_err());
    }

    #[test]
    fn resolve_rejects_what_is_missing() {
        for mode in [Mode::Test, Mode::Sleep] {
            let c = CanConfig {
                mode,
                ..config(None)
            };
            assert_eq!(resolve(&c, &H7), Err(unsupported()));
        }
        for o in [
            Options {
                presume_ack: true,
                ..options()
            },
            Options {
                triple_sample: true,
                ..options()
            },
            Options {
                tx_order: TxOrder::Priority,
                ..options()
            },
            Options {
                rx_overflow: RxOverflow::KeepNewest,
                ..options()
            },
            Options {
                wake_on_bus: true,
                ..options()
            },
        ] {
            let c = CanConfig {
                options: o,
                ..config(None)
            };
            assert_eq!(resolve(&c, &H7), Err(unsupported()));
        }
        let c = CanConfig {
            bus_off_recovery: None,
            ..config(None)
        };
        assert_eq!(resolve(&c, &H7).unwrap().recovery, Recovery::Manual);
        let c = CanConfig {
            bus_off_recovery: Some(Second::from_parts(Prefix::Unit, NumericValue::U8(0))),
            ..config(None)
        };
        assert_eq!(resolve(&c, &H7).unwrap().recovery, Recovery::Immediate);
    }

    #[test]
    fn default_is_250k() {
        let hw = default_config(&H7).unwrap();
        assert_eq!(hw.nominal.bit_rate(CLOCK_HZ), Some(250_000));
        assert_eq!(hw.nominal.sample_point(), 875);
        assert_eq!(resolve(&hw.to_config(), &H7).unwrap(), hw);
    }

    fn std(id: u16) -> CanId {
        CanId::standard(id).unwrap()
    }

    fn ext(id: u32) -> CanId {
        CanId::extended(id).unwrap()
    }

    fn fifo(fifo: u8) -> FilterAction {
        FilterAction::Fifo {
            fifo,
            high_priority: false,
        }
    }

    fn filters(list: &[Filter]) -> FilterConfig<'_> {
        FilterConfig {
            filters: MaxLen::new_unchecked(RefVec::Slice { slice: list }),
            non_matching_standard: NonMatching::Reject,
            non_matching_extended: NonMatching::Accept { fifo: 1 },
            reject_remote: true,
        }
    }

    #[test]
    fn filters_map_in_order() {
        let list = [
            Filter {
                matching: FilterMatch::Exact(std(0x100)),
                action: fifo(0),
            },
            Filter {
                matching: FilterMatch::Range {
                    from: ext(0x1000),
                    to: ext(0x1FFF),
                },
                action: FilterAction::Reject,
            },
            Filter {
                matching: FilterMatch::Mask {
                    id: std(0x200),
                    mask: U29::new(0x7F0).unwrap(),
                },
                action: FilterAction::Fifo {
                    fifo: 1,
                    high_priority: true,
                },
            },
            Filter {
                matching: FilterMatch::Range {
                    from: ext(0x1800),
                    to: ext(0x18FF),
                },
                action: fifo(0),
            },
        ];
        let hw = HwFilters::map(&filters(&list), &H7).unwrap();
        assert_eq!(
            hw.standard[0],
            Some(HwFilter {
                matching: HwMatch::Dual(0x100, 0x100),
                action: HwAction::Fifo {
                    fifo: 0,
                    priority: false
                }
            })
        );
        assert_eq!(
            hw.standard[1].unwrap().matching,
            HwMatch::Mask {
                id: 0x200,
                mask: 0x7F0
            }
        );
        assert_eq!(hw.standard[2], None);
        assert_eq!(hw.extended[0].unwrap().action, HwAction::Reject);
        assert_eq!(
            hw.extended[1].unwrap().matching,
            HwMatch::Range {
                from: 0x1800,
                to: 0x18FF
            }
        );
        assert_eq!(hw.non_matching_standard, None);
        assert_eq!(hw.non_matching_extended, Some(1));
        assert!(hw.reject_remote);

        // element indices (FIDX) back to the host's list
        assert_eq!(hw.host_index(false, 0), Some(0));
        assert_eq!(hw.host_index(false, 1), Some(2));
        assert_eq!(hw.host_index(false, 2), None);
        assert_eq!(hw.host_index(true, 0), Some(1));
        assert_eq!(hw.host_index(true, 1), Some(3));
        assert_eq!(hw.host_index(true, 2), None);
        assert_eq!(hw.host_index(false, 200), None);
        assert_eq!(HwFilters::accept_all().host_index(false, 0), None);
    }

    #[test]
    fn filters_rejected() {
        let one = |matching, action| [Filter { matching, action }];
        let bad = [
            one(FilterMatch::Exact(std(1)), FilterAction::Buffer(0)),
            one(FilterMatch::Exact(std(1)), fifo(2)),
            one(
                FilterMatch::Mask {
                    id: std(1),
                    mask: U29::new(0xFFF).unwrap(),
                },
                fifo(0),
            ),
            one(
                FilterMatch::Range {
                    from: std(5),
                    to: std(4),
                },
                fifo(0),
            ),
            one(
                FilterMatch::Range {
                    from: std(5),
                    to: ext(6),
                },
                fifo(0),
            ),
        ];
        for list in &bad {
            assert!(HwFilters::map(&filters(list), &H7).is_err(), "{list:?}");
        }
        let many = [Filter {
            matching: FilterMatch::Exact(ext(1)),
            action: fifo(0),
        }; EXTENDED_FILTERS + 1];
        assert_eq!(
            HwFilters::map(&filters(&many), &H7),
            Err(CanError::OutOfFilters {
                max: EXTENDED_FILTERS as u16
            })
        );
        let many = [Filter {
            matching: FilterMatch::Exact(std(1)),
            action: fifo(0),
        }; STANDARD_FILTERS + 1];
        assert_eq!(
            HwFilters::map(&filters(&many), &H7),
            Err(CanError::OutOfFilters {
                max: STANDARD_FILTERS as u16
            })
        );
        let mut c = filters(&[]);
        c.non_matching_standard = NonMatching::Accept { fifo: 2 };
        assert!(HwFilters::map(&c, &H7).is_err());
    }

    fn frame(id: CanId, kind: FrameKind, data: &[u8]) -> Frame<'_> {
        Frame {
            id,
            kind,
            data: MaxLen::new_unchecked(RefVec::new_bytes(data)),
        }
    }

    #[test]
    fn tx_frames() {
        let p = tx_plan(&frame(std(0x123), FrameKind::Data, &[1, 2, 3]), false).unwrap();
        assert_eq!(
            (p.extended, p.id, p.fd, p.rtr, p.len),
            (false, 0x123, false, false, 3)
        );
        let p = tx_plan(
            &frame(
                ext(0x1234567),
                FrameKind::Remote {
                    len: U4::new(8).unwrap(),
                },
                &[],
            ),
            false,
        )
        .unwrap();
        assert_eq!((p.extended, p.rtr, p.len), (true, true, 8));
        let fd = FrameKind::Fd {
            brs: true,
            esi: false,
        };
        let p = tx_plan(&frame(std(1), fd, &[0; 13]), true).unwrap();
        assert_eq!((p.fd, p.brs, p.len), (true, true, 16));
        assert_eq!(
            tx_plan(&frame(std(1), fd, &[0; 13]), false),
            Err(TxReject::FdOff)
        );
        assert_eq!(
            tx_plan(&frame(std(1), FrameKind::Data, &[0; 9]), true),
            Err(TxReject::BadLength)
        );
        assert_eq!(
            tx_plan(
                &frame(
                    std(1),
                    FrameKind::Remote {
                        len: U4::new(9).unwrap()
                    },
                    &[]
                ),
                true
            ),
            Err(TxReject::BadLength)
        );
        assert_eq!(
            tx_plan(
                &frame(
                    std(1),
                    FrameKind::DataLongDlc {
                        dlc: U4::new(9).unwrap()
                    },
                    &[0; 8]
                ),
                true
            ),
            Err(TxReject::Unsupported)
        );
    }

    #[test]
    fn fd_lengths() {
        assert_eq!(fd_len(0), Some(0));
        assert_eq!(fd_len(8), Some(8));
        assert_eq!(fd_len(9), Some(12));
        assert_eq!(fd_len(33), Some(48));
        assert_eq!(fd_len(64), Some(64));
        assert_eq!(fd_len(65), None);
    }

    #[test]
    fn status_and_codes() {
        let hw = HwState {
            passive: true,
            warning: true,
            tec: 130,
            ..HwState::default()
        };
        assert_eq!(
            status(Mode::Normal, hw, None, None).state,
            ErrorState::Passive
        );
        assert_eq!(
            status(Mode::Config, hw, None, None).state,
            ErrorState::Stopped
        );
        let off = HwState {
            bus_off: true,
            ..hw
        };
        assert_eq!(
            status(Mode::ListenOnly, off, None, None).state,
            ErrorState::BusOff
        );
        assert_eq!(
            status(Mode::Normal, HwState::default(), None, None).state,
            ErrorState::Active
        );
        assert_eq!(protocol_error(3), Some(ProtocolError::Ack));
        assert_eq!(protocol_error(0), None);
        assert_eq!(protocol_error(7), None);
        assert_eq!(activity(3), Activity::Transmitting);
        assert!(mode_sends(Mode::InternalLoopback) && !mode_sends(Mode::Restricted));
    }

    #[test]
    fn capabilities_match_limits() {
        let c = capabilities(&H7, 256, 32, &[]);
        assert_eq!(c.clock.to_u32_in(Prefix::Unit), Some(CLOCK_HZ));
        assert_eq!(c.termination.len(), 0);
        assert_eq!(c.standard_filters, 64);
        assert_eq!(c.extended_filters, 32);
    }
    #[test]
    fn g0_global_filter_only() {
        // G0B1 at 64 MHz PCLK: 250 kbit/s exact
        let g0 = Controller::g0(64_000_000);
        let hw = default_config(&g0).unwrap();
        assert_eq!(hw.nominal.bit_rate(g0.clock_hz), Some(250_000));
        assert_eq!(hw.nominal.sample_point(), 875);
        let c = capabilities(&g0, 64, 32, &[]);
        assert_eq!((c.standard_filters, c.extended_filters), (0, 0));
        assert!(!c.filter_mask && !c.filter_range);
        // no elements: the global filter alone maps, any element is out of filters
        let hw = HwFilters::map(&filters(&[]), &g0).unwrap();
        assert_eq!(hw.non_matching_extended, Some(1));
        let one = [Filter {
            matching: FilterMatch::Exact(std(1)),
            action: fifo(0),
        }];
        assert_eq!(
            HwFilters::map(&filters(&one), &g0),
            Err(CanError::OutOfFilters { max: 0 })
        );
    }

    #[test]
    fn h7_layouts_fit_the_message_ram() {
        // words: a standard filter 1, an extended one 2, an element with 64 data bytes 18
        let words = |c: &Controller| {
            c.standard_filters as u32
                + 2 * c.extended_filters as u32
                + 18 * (c.rx_fifo0 as u32 + c.rx_fifo1 as u32 + c.tx_fifo as u32)
        };
        assert_eq!(words(&H7), 2432);
        assert!(2 * words(&Controller::h7_half(CLOCK_HZ)) <= 2560);
        assert!(words(&H7) + words(&Controller::h7_half(CLOCK_HZ)) > 2560);
    }
}
