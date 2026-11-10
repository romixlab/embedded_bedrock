//! The G0 field positions of `bedrock_clocks::g0::regs::field` and the addresses and bits of `bedrock_clocks_rt::g0`
//! against stm32-metapac's STM32G0B1CE, field by field. Its own pass, as metapac takes one chip per build:
//! `cargo test -p bedrock_clocks --no-default-features --features metapac-check-g0 --test g0_metapac`.

use bedrock_clocks::g0::regs::field::{self, put};
use bedrock_clocks_rt::g0::{addr, bits};
use stm32_metapac::crs::regs as cr;
use stm32_metapac::flash::regs as fr;
use stm32_metapac::pwr::{regs as pr, vals as pv};
use stm32_metapac::rcc::{regs as rr, vals as rv};

#[test]
fn addresses() {
    let a = |p: *mut u8| p as u32;
    assert_eq!(a(stm32_metapac::RCC.as_ptr() as _), addr::RCC);
    assert_eq!(a(stm32_metapac::PWR.as_ptr() as _), addr::PWR);
    assert_eq!(a(stm32_metapac::FLASH.as_ptr() as _), addr::FLASH);
    assert_eq!(a(stm32_metapac::CRS.as_ptr() as _), addr::CRS);
    let rcc = stm32_metapac::RCC;
    assert_eq!(rcc.cr().as_ptr() as u32, addr::RCC_CR);
    assert_eq!(rcc.cfgr().as_ptr() as u32, addr::RCC_CFGR);
    assert_eq!(rcc.pllcfgr().as_ptr() as u32, addr::RCC_PLLCFGR);
    assert_eq!(rcc.apbenr1().as_ptr() as u32, addr::RCC_APBENR1);
    assert_eq!(rcc.ccipr().as_ptr() as u32, addr::RCC_CCIPR);
    assert_eq!(rcc.ccipr2().as_ptr() as u32, addr::RCC_CCIPR2);
    assert_eq!(stm32_metapac::PWR.cr1().as_ptr() as u32, addr::PWR_CR1);
    assert_eq!(stm32_metapac::PWR.sr2().as_ptr() as u32, addr::PWR_SR2);
    assert_eq!(stm32_metapac::FLASH.acr().as_ptr() as u32, addr::FLASH_ACR);
    assert_eq!(stm32_metapac::CRS.cr().as_ptr() as u32, addr::CRS_CR);
    assert_eq!(stm32_metapac::CRS.cfgr().as_ptr() as u32, addr::CRS_CFGR);
}

#[test]
fn pwr_and_flash() {
    let mut r = pr::Cr1(0);
    r.set_vos(pv::Vos::RANGE2);
    assert_eq!(r.0, put(field::PWR_CR1_VOS, 0b10));
    assert_eq!(r.0, 0b10 << 9);
    let mut r = pr::Cr1(0);
    r.set_vos(pv::Vos::from_bits(0b11));
    assert_eq!(r.0, bits::PWR_CR1_VOS);
    let mut s = pr::Sr2(0);
    s.set_vosf(true);
    assert_eq!(s.0, bits::PWR_SR2_VOSF);

    let mut a = fr::Acr(0);
    a.set_latency(stm32_metapac::flash::vals::Latency::WS2);
    assert_eq!(a.0, put(field::FLASH_ACR_LATENCY, 2));
    let mut a = fr::Acr(0);
    a.set_prften(true);
    assert_eq!(a.0, field::FLASH_ACR_PRFTEN);
    let mut a = fr::Acr(0);
    a.set_icen(true);
    assert_eq!(a.0, field::FLASH_ACR_ICEN);
    let mut a = fr::Acr(0);
    a.set_latency(stm32_metapac::flash::vals::Latency::from_bits(0b111));
    a.set_prften(true);
    a.set_icen(true);
    assert_eq!(a.0, bits::ACR_PLAN);
}

#[test]
fn rcc_cr_cfgr_apbenr1() {
    for (f, set) in [
        (
            field::CR_HSION,
            &(|r: &mut rr::Cr| r.set_hsion(true)) as &dyn Fn(&mut rr::Cr),
        ),
        (field::CR_HSIRDY, &|r| r.set_hsirdy(true)),
        (field::CR_HSEON, &|r| r.set_hseon(true)),
        (field::CR_HSERDY, &|r| r.set_hserdy(true)),
        (field::CR_HSEBYP, &|r| r.set_hsebyp(true)),
        (field::CR_HSI48ON, &|r| r.set_hsi48on(true)),
        (field::CR_HSI48RDY, &|r| r.set_hsi48rdy(true)),
        (field::CR_PLLON, &|r| r.set_pllon(true)),
        (field::CR_PLLRDY, &|r| r.set_pllrdy(true)),
    ] {
        let mut r = rr::Cr(0);
        set(&mut r);
        assert_eq!(r.0, f);
    }
    let mut r = rr::Cr(0);
    r.set_hsidiv(rv::Hsidiv::from_bits(0b111));
    assert_eq!(r.0, put(field::CR_HSIDIV, 0b111));
    assert_eq!(r.0, bits::CR_HSIDIV);
    // the runtime's bits are the planner's
    assert_eq!(bits::CR_HSION, field::CR_HSION);
    assert_eq!(bits::CR_HSEBYP, field::CR_HSEBYP);
    assert_eq!(bits::CR_HSI48ON, field::CR_HSI48ON);
    assert_eq!(bits::CR_PLLON, field::CR_PLLON);

    let mut c = rr::Cfgr(0);
    c.set_sw(rv::Sw::from_bits(0b111));
    assert_eq!(c.0, put(field::CFGR_SW, 0b111));
    assert_eq!(c.0, bits::CFGR_SW);
    let mut c = rr::Cfgr(0);
    c.set_sws(rv::Sw::from_bits(0b111));
    assert_eq!(c.0, put(field::CFGR_SWS, 0b111));
    let mut c = rr::Cfgr(0);
    c.set_hpre(rv::Hpre::DIV512);
    assert_eq!(c.0, put(field::CFGR_HPRE, 0b1111));
    let mut c = rr::Cfgr(0);
    c.set_hpre(rv::Hpre::DIV64);
    assert_eq!(
        c.0,
        put(
            field::CFGR_HPRE,
            bedrock_clocks::g0::Hpre::from_ratio(64)
                .unwrap()
                .bits()
                .into()
        )
    );
    let mut c = rr::Cfgr(0);
    c.set_ppre(rv::Ppre::DIV16);
    assert_eq!(c.0, put(field::CFGR_PPRE, 0b111));
    let mut c = rr::Cfgr(0);
    c.set_ppre(rv::Ppre::DIV2);
    assert_eq!(
        c.0,
        put(
            field::CFGR_PPRE,
            bedrock_clocks::g0::Ppre::from_ratio(2)
                .unwrap()
                .bits()
                .into()
        )
    );

    let mut e = rr::Apbenr1(0);
    e.set_pwren(true);
    assert_eq!(e.0, bits::APBENR1_PWREN);
    let mut e = rr::Apbenr1(0);
    e.set_crsen(true);
    assert_eq!(e.0, bits::APBENR1_CRSEN);
}

#[test]
fn pllcfgr() {
    let mut p = rr::Pllcfgr(0);
    p.set_pllsrc(rv::Pllsrc::HSI);
    assert_eq!(p.0, put(field::PLLCFGR_PLLSRC, 0b10));
    let mut p = rr::Pllcfgr(0);
    p.set_pllm(rv::Pllm::from_bits(0b111));
    assert_eq!(p.0, put(field::PLLCFGR_PLLM, 0b111));
    let mut p = rr::Pllcfgr(0);
    p.set_plln(rv::Plln::from_bits(86));
    assert_eq!(p.0, put(field::PLLCFGR_PLLN, 86));
    let mut p = rr::Pllcfgr(0);
    p.set_pllp(rv::Pllp::from_bits(31));
    assert_eq!(p.0, put(field::PLLCFGR_PLLP, 31));
    let mut p = rr::Pllcfgr(0);
    p.set_pllq(rv::Pllq::from_bits(7));
    assert_eq!(p.0, put(field::PLLCFGR_PLLQ, 7));
    let mut p = rr::Pllcfgr(0);
    p.set_pllr(rv::Pllr::from_bits(7));
    assert_eq!(p.0, put(field::PLLCFGR_PLLR, 7));
    let mut p = rr::Pllcfgr(0);
    p.set_pllpen(true);
    assert_eq!(p.0, field::PLLCFGR_PLLPEN);
    let mut p = rr::Pllcfgr(0);
    p.set_pllqen(true);
    assert_eq!(p.0, field::PLLCFGR_PLLQEN);
    let mut p = rr::Pllcfgr(0);
    p.set_pllren(true);
    assert_eq!(p.0, field::PLLCFGR_PLLREN);
}

#[test]
fn ccipr_ccipr2() {
    let mut c = rr::Ccipr(0);
    c.set_usart1sel(rv::Usartsel::from_bits(3));
    c.set_usart2sel(rv::Usartsel::from_bits(3));
    c.set_usart3sel(rv::Usartsel::from_bits(3));
    c.set_lpuart2sel(rv::Lpuart2sel::from_bits(3));
    c.set_lpuart1sel(rv::Lpuart1sel::from_bits(3));
    c.set_i2c1sel(rv::I2c1sel::from_bits(3));
    c.set_i2c2i2s1sel(rv::I2c2i2s1sel::from_bits(3));
    c.set_lptim1sel(rv::Lptim1sel::from_bits(3));
    c.set_lptim2sel(rv::Lptim2sel::from_bits(3));
    c.set_tim1sel(rv::Tim1sel::from_bits(1));
    c.set_tim15sel(rv::Tim15sel::from_bits(1));
    c.set_rngsel(rv::Rngsel::from_bits(3));
    c.set_rngdiv(rv::Rngdiv::from_bits(3));
    c.set_adcsel(rv::Adcsel::from_bits(3));
    let mut all = 0;
    for f in [
        field::CCIPR_USART1SEL,
        field::CCIPR_USART2SEL,
        field::CCIPR_USART3SEL,
        field::CCIPR_LPUART2SEL,
        field::CCIPR_LPUART1SEL,
        field::CCIPR_I2C1SEL,
        field::CCIPR_I2C2I2S1SEL,
        field::CCIPR_LPTIM1SEL,
        field::CCIPR_LPTIM2SEL,
        field::CCIPR_TIM1SEL,
        field::CCIPR_TIM15SEL,
        field::CCIPR_RNGSEL,
        field::CCIPR_RNGDIV,
        field::CCIPR_ADCSEL,
    ] {
        all |= put(f, u32::MAX);
    }
    assert_eq!(c.0, all);
    assert_eq!(c.0, bits::CCIPR_PLAN);
    // one field at a time, so two swapped fields of the same width show
    let mut c = rr::Ccipr(0);
    c.set_usart3sel(rv::Usartsel::from_bits(1));
    assert_eq!(c.0, put(field::CCIPR_USART3SEL, 1));
    let mut c = rr::Ccipr(0);
    c.set_lpuart2sel(rv::Lpuart2sel::from_bits(1));
    assert_eq!(c.0, put(field::CCIPR_LPUART2SEL, 1));
    let mut c = rr::Ccipr(0);
    c.set_i2c2i2s1sel(rv::I2c2i2s1sel::from_bits(1));
    assert_eq!(c.0, put(field::CCIPR_I2C2I2S1SEL, 1));
    let mut c = rr::Ccipr(0);
    c.set_lptim2sel(rv::Lptim2sel::from_bits(1));
    assert_eq!(c.0, put(field::CCIPR_LPTIM2SEL, 1));
    let mut c = rr::Ccipr(0);
    c.set_tim15sel(rv::Tim15sel::from_bits(1));
    assert_eq!(c.0, put(field::CCIPR_TIM15SEL, 1));
    let mut c = rr::Ccipr(0);
    c.set_rngdiv(rv::Rngdiv::from_bits(1));
    assert_eq!(c.0, put(field::CCIPR_RNGDIV, 1));
    let mut c = rr::Ccipr(0);
    c.set_adcsel(rv::Adcsel::from_bits(1));
    assert_eq!(c.0, put(field::CCIPR_ADCSEL, 1));

    let mut c = rr::Ccipr2(0);
    c.set_i2s1sel(rv::I2s1sel::from_bits(1));
    assert_eq!(c.0, put(field::CCIPR2_I2S1SEL, 1));
    let mut c = rr::Ccipr2(0);
    c.set_i2s2sel(rv::I2s2sel::from_bits(1));
    assert_eq!(c.0, put(field::CCIPR2_I2S2SEL, 1));
    let mut c = rr::Ccipr2(0);
    c.set_fdcansel(rv::Fdcansel::from_bits(1));
    assert_eq!(c.0, put(field::CCIPR2_FDCANSEL, 1));
    let mut c = rr::Ccipr2(0);
    c.set_usbsel(rv::Usbsel::from_bits(1));
    assert_eq!(c.0, put(field::CCIPR2_USBSEL, 1));
    let mut c = rr::Ccipr2(0);
    c.set_i2s1sel(rv::I2s1sel::from_bits(3));
    c.set_i2s2sel(rv::I2s2sel::from_bits(3));
    c.set_fdcansel(rv::Fdcansel::from_bits(3));
    c.set_usbsel(rv::Usbsel::from_bits(3));
    assert_eq!(c.0, bits::CCIPR2_PLAN);
}

#[test]
fn crs() {
    let mut c = cr::Cr(0);
    c.set_autotrimen(true);
    assert_eq!(c.0, field::CRS_CR_AUTOTRIMEN);
    assert_eq!(c.0, bits::CRS_CR_AUTOTRIMEN);
    let mut c = cr::Cr(0);
    c.set_cen(true);
    assert_eq!(c.0, field::CRS_CR_CEN);
    // TRIM is 7 bits on the G0B1 (RM0444 Table 37, p.221: reset 64, CRS_CR 0x0000_4000); metapac's crs_v1 has 6 and
    // can't hold 64, so only the position is held to it
    let mut c = cr::Cr(0);
    c.set_trim(1);
    assert_eq!(c.0, put(field::CRS_CR_TRIM, 1));
    assert_eq!(put(field::CRS_CR_TRIM, 64), 0x4000);
    let mut g = cr::Cfgr(0);
    g.set_reload(47_999);
    g.set_felim(34);
    g.set_syncsrc(stm32_metapac::crs::vals::Syncsrc::USB);
    assert_eq!(
        g.0,
        put(field::CRS_CFGR_RELOAD, 47_999)
            | put(field::CRS_CFGR_FELIM, 34)
            | put(field::CRS_CFGR_SYNCSRC, 0b10)
    );
    // the reset value, as RM0444 §6.7.2 gives it
    assert_eq!(g.0, 0x2022_BB7F);
    let mut g = cr::Cfgr(0);
    g.set_syncdiv(7);
    assert_eq!(g.0, put(field::CRS_CFGR_SYNCDIV, 7));
}
