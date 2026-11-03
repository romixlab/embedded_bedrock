//! `EXE-5` Interrupt executor on a vector that a driver also binds or that a used peripheral raises

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;

pub struct Exe5;

static META: CheckMeta = CheckMeta {
    id: "EXE-5",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Exe5));

const FIX: &str = "start the executor on the vector of a peripheral the firmware doesn't use (a software interrupt such as SWI_IRQ_n on RP, or an unused timer or serial port's vector on STM32), and say so in a comment";

/// The peripherals a vector's name stands for: `TIM7_LPTIM2` → TIM7, LPTIM2; `USART3_4_5_6_LPUART1` → USART3,
/// USART4, USART5, USART6, LPUART1; `DMA1_CHANNEL2_3` → DMA1, DMA1_CH2, DMA1_CH3
pub fn vector_peripherals(vector: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut prefix = String::new();
    let mut base = String::new();
    for part in vector.split('_') {
        if part.is_empty() {
            continue;
        }
        if part.chars().all(|c| c.is_ascii_digit()) {
            if !prefix.is_empty() {
                out.push(format!("{prefix}{part}"));
            }
            continue;
        }
        if let Some(n) = part.strip_prefix("CHANNEL")
            && !base.is_empty()
        {
            prefix = format!("{base}_CH");
            if !n.is_empty() {
                out.push(format!("{prefix}{n}"));
            }
            continue;
        }
        prefix = part
            .trim_end_matches(|c: char| c.is_ascii_digit())
            .to_string();
        if part != prefix {
            base = part.to_string();
        }
        out.push(part.to_string());
    }
    out.push(vector.to_string());
    out.retain(|p| !p.starts_with("IRQ") && p != "SWI");
    out.dedup();
    out
}

impl Check for Exe5 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// For each interrupt executor start: a `bind_interrupts!` line for its vector is certain; a peripheral the
    /// vector's name stands for that the source uses (`p.TIM7`, `peripherals::TIM7`) is an estimate (the
    /// peripheral may not raise that interrupt)
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let facts = &fw.facts;
        let mut out = Vec::new();
        for s in &facts.executor_starts {
            if s.vector.is_empty() {
                continue;
            }
            let item = format!("{}:{}", s.in_fn, s.executor);
            let start = s
                .loc
                .span()
                .label(format!("{}.start({})", s.executor, s.vector));
            let bound: Vec<_> = facts
                .bindings
                .iter()
                .filter(|b| b.vector == s.vector)
                .collect();
            if !bound.is_empty() {
                let mut f = Finding::new(
                    "EXE-5",
                    item,
                    format!(
                        "interrupt executor {} runs on {}, which bind_interrupts! also gives to {}: the driver's interrupts poll the executor and the executor's wake-ups run the driver's handler",
                        s.executor, s.vector, bound[0].handlers
                    ),
                )
                .span(start)
                .fix(FIX);
                for b in &bound {
                    f = f.span(b.loc.span().label(format!("bound to {}", b.handlers)));
                }
                out.push(f);
                continue;
            }
            let names = vector_peripherals(&s.vector);
            let used: Vec<_> = facts
                .peripherals
                .iter()
                .filter(|p| names.contains(&p.name))
                .collect();
            if let Some(first) = used.first() {
                out.push(
                    Finding::new(
                        "EXE-5",
                        item,
                        format!(
                            "interrupt executor {} runs on {}, and the firmware uses {}: when that peripheral raises the interrupt, it polls the executor and the peripheral's flags are never cleared",
                            s.executor, s.vector, first.name
                        ),
                    )
                    .span(start)
                    .span(first.loc.span().label(format!("{} used here", first.name)))
                    .fix(FIX)
                    .confidence(Confidence::Estimate),
                );
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::vector_peripherals;

    #[test]
    fn peripherals_of_vectors() {
        assert_eq!(
            vector_peripherals("TIM7_LPTIM2"),
            ["TIM7", "LPTIM2", "TIM7_LPTIM2"]
        );
        assert_eq!(
            vector_peripherals("USART3_4_5_6_LPUART1"),
            [
                "USART3",
                "USART4",
                "USART5",
                "USART6",
                "LPUART1",
                "USART3_4_5_6_LPUART1"
            ]
        );
        assert_eq!(
            vector_peripherals("DMA1_CHANNEL2_3"),
            ["DMA1", "DMA1_CH2", "DMA1_CH3", "DMA1_CHANNEL2_3"]
        );
        assert_eq!(vector_peripherals("SWI_IRQ_0"), ["SWI_IRQ_0"]);
        assert_eq!(vector_peripherals("PIO0_IRQ_0"), ["PIO0", "PIO0_IRQ_0"]);
    }
}
