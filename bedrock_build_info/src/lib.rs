use wire_weaver::date_time::DateTime;
use wire_weaver::derive_shrink_wrap;
use wire_weaver::shrink_wrap::nib32::UNib32;
use wire_weaver::shrink_wrap::prelude::*;

#[derive_shrink_wrap]
#[derive(Debug, PartialEq, Eq)]
#[shrink_wrap(no_alloc)]
pub struct BedrockBuildInfo<'i> {
    /// Firmware build time
    pub timestamp: DateTime,
    #[final_evolution]
    pub profile: Profile,
    #[final_evolution]
    pub optimization_level: OptimizationLevel,
    #[flag]
    version_control: bool,
    pub crate_info: CrateInfo<'i>,
    pub target_info: TargetInfo<'i>,
    pub compiler_info: CompilerInfo<'i>,
    pub version_control: Option<VersionControl<'i>>,
}

#[derive_shrink_wrap]
#[derive(Debug, PartialEq, Eq)]
#[ww_repr(u2)]
pub enum Profile {
    Release,
    Debug,
}

#[derive_shrink_wrap]
#[derive(Debug, PartialEq, Eq)]
#[ww_repr(u3)]
pub enum OptimizationLevel {
    O0,
    O1,
    O2,
    O3,
    Os,
    Oz,
}

#[derive_shrink_wrap]
#[derive(Debug, PartialEq, Eq, Clone)]
#[shrink_wrap(no_alloc)]
pub struct CrateInfo<'i> {
    pub name: &'i str,
    pub version: Version,
    pub authors: RefVec<'i, &'i str>,
    pub enabled_features: RefVec<'i, &'i str>,
    pub dependencies: RefVec<'i, CrateInfo<'i>>,
}

#[derive_shrink_wrap]
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct Version {
    pub major: UNib32,
    pub minor: UNib32,
    pub patch: UNib32,
}

#[derive_shrink_wrap]
#[derive(Debug, PartialEq, Eq)]
#[shrink_wrap(no_alloc)]
pub struct TargetInfo<'i> {
    #[flag]
    arch: bool,
    pub triple: Option<&'i str>,
    pub arch: Option<&'i str>,
}

#[derive_shrink_wrap]
#[derive(Debug, PartialEq, Eq)]
#[shrink_wrap(no_alloc)]
pub struct CompilerInfo<'i> {
    pub version: Version,
    #[final_evolution]
    pub channel: CompilerChannel,
    pub host_triple: Option<&'i str>,
    /// Compiler build time
    pub commit_date: DateTime,
}

#[derive_shrink_wrap]
#[derive(Debug, PartialEq, Eq)]
#[ww_repr(u3)]
pub enum CompilerChannel {
    Dev,
    Nightly,
    Beta,
    Stable,
}

#[derive_shrink_wrap]
#[derive(Debug, PartialEq, Eq)]
#[shrink_wrap(no_alloc)]
pub struct VersionControl<'i> {
    #[flag]
    commit_id: bool,
    #[flag]
    commit_short_id: bool,
    #[flag]
    branch: bool,
    pub dirty: bool,
    pub commit_id: Option<&'i str>,
    pub commit_short_id: Option<&'i str>,
    pub commit_timestamp: DateTime,
    pub branch: Option<&'i str>,
    pub tags: RefVec<'i, &'i str>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;
    use wire_weaver::date_time::{NaiveDate, NaiveTime, Year};

    #[test]
    fn v1_version_compatibility_not_broken() {
        let build_info = BedrockBuildInfo {
            timestamp: DateTime {
                date: NaiveDate {
                    year: Year::new(2025).unwrap(),
                    month: U4::new(7).unwrap(),
                    day: U5::new(13).unwrap(),
                },
                time: NaiveTime {
                    secs: U17::new(58_800).unwrap(),
                    frac: None,
                },
                offset: None,
            },
            profile: Profile::Release,
            optimization_level: OptimizationLevel::O2,
            crate_info: CrateInfo {
                name: "awesome",
                version: Version {
                    major: UNib32(0),
                    minor: UNib32(1),
                    patch: UNib32(2),
                },
                authors: RefVec::empty(),
                enabled_features: RefVec::new_str_slice(&["f_a", "f_b"]),
                dependencies: RefVec::empty(),
            },
            target_info: TargetInfo {
                triple: Some("xyz"),
                arch: Some("arm"),
            },
            compiler_info: CompilerInfo {
                version: Version {
                    major: UNib32(1),
                    minor: UNib32(87),
                    patch: UNib32(0),
                },
                channel: CompilerChannel::Nightly,
                host_triple: None,
                commit_date: DateTime {
                    date: NaiveDate {
                        year: Year::new(2025).unwrap(),
                        month: U4::new(5).unwrap(),
                        day: U5::new(5).unwrap(),
                    },
                    time: NaiveTime {
                        secs: U17::new(58_800).unwrap(),
                        frac: None,
                    },
                    offset: None,
                },
            },
            version_control: Some(VersionControl {
                dirty: true,
                commit_id: None,
                commit_short_id: Some("abc"),
                commit_timestamp: DateTime {
                    date: NaiveDate {
                        year: Year::new(2025).unwrap(),
                        month: U4::new(7).unwrap(),
                        day: U5::new(14).unwrap(),
                    },
                    time: NaiveTime {
                        secs: U17::new(58_800).unwrap(),
                        frac: None,
                    },
                    offset: None,
                },
                branch: None,
                tags: RefVec::empty(),
            }),
        };
        let mut buf = [0u8; 256];
        let mut wr = BufWriter::new(&mut buf);
        build_info.ser_shrink_wrap(&mut wr).unwrap();
        let bytes = wr.finish_and_take().unwrap();
        // println!("{}: {bytes:02X?}", bytes.len());
        assert_eq!(bytes.len(), 53);
        assert_eq!(
            bytes,
            hex!(
                "076B96C0 14 617765736F6D65" // DateTime profile,optimization,version_control_flag "awesome"
                "0120 665F61 665F62 00332027" // Version "f_a" "f_b" lengths: 7, 2, 0, 2, 3, 3, 0, free nib
                "C0 78797A 61726D 33" // TargetInfo: flags "xyz" "arm" lengths: 3, 3
                "1FA100 2052B96C 03" // CompilerInfo: Version: 1,87,0 channel host_triple DateTime Version length=3
                "50 616263 077396C0 03" // VersionControl: flags "abc" DateTime str len = 3, vec len = 0
                "19 09 09 3A" // lengths from the back: 19, 8, 8, 9
            )
        );
        let mut rd = BufReader::new(&bytes[..]);
        let build_info_des: BedrockBuildInfo = rd.read(ElementSize::Implied).unwrap();
        assert_eq!(build_info, build_info_des);
    }
}
