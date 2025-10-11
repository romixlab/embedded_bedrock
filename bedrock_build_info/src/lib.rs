#![cfg_attr(not(feature = "std"), no_std)]

pub mod traits;

use wire_weaver::derive_shrink_wrap;
use wire_weaver::shrink_wrap::prelude::*;
use ww_date_time::{DateTime, NaiveDate};
use ww_version::Version;
#[cfg(feature = "std")]
use ww_version::VersionOwned;

pub const COMPACT_INFO_MAGIC: u32 = 0xB17D_14F0;

#[cfg(feature = "std")]
pub fn build_info_crc(bytes: &[u8]) -> u32 {
    let crc = crc::Crc::<u32>::new(&crc::CRC_32_BZIP2);
    let mut crc = crc.digest();
    crc.update(bytes);
    crc.finalize()
}

#[derive_shrink_wrap]
#[derive(Debug, PartialEq, Eq)]
#[shrink_wrap(no_alloc)]
#[owned = "std"]
pub struct BedrockBuildInfo<'i> {
    /// Firmware build time
    pub timestamp: DateTime,
    pub profile: Profile,
    pub optimization_level: OptimizationLevel,
    #[flag]
    version_control: bool,
    pub crate_info: CrateInfo<'i>,
    pub target_info: TargetInfo<'i>,
    pub compiler_info: CompilerInfo<'i>,
    pub version_control: Option<VersionControl<'i>>,
}

#[derive_shrink_wrap]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[ww_repr(u2)]
#[sized]
pub enum Profile {
    Release,
    Debug,
}

#[derive_shrink_wrap]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[ww_repr(u3)]
#[sized]
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
#[owned = "std"]
pub struct CrateInfo<'i> {
    pub name: &'i str,
    pub version: Version<'i>,
    pub authors: RefVec<'i, &'i str>,
    pub enabled_features: RefVec<'i, &'i str>,
    pub dependencies: RefVec<'i, CrateInfo<'i>>,
}

#[derive_shrink_wrap]
#[derive(Debug, PartialEq, Eq)]
#[shrink_wrap(no_alloc)]
#[owned = "std"]
pub struct TargetInfo<'i> {
    #[flag]
    arch: bool,
    pub triple: Option<&'i str>,
    pub arch: Option<&'i str>,
}

#[derive_shrink_wrap]
#[derive(Debug, PartialEq, Eq)]
#[shrink_wrap(no_alloc)]
#[owned = "std"]
pub struct CompilerInfo<'i> {
    pub version: Version<'i>,
    pub channel: CompilerChannel,
    pub host_triple: Option<&'i str>,
    /// Compiler build time
    pub commit_date: Option<NaiveDate>,
    pub flip_link: bool,
}

#[derive_shrink_wrap]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[ww_repr(u3)]
#[sized]
pub enum CompilerChannel {
    Dev,
    Nightly,
    Beta,
    Stable,
}

#[derive_shrink_wrap]
#[derive(Debug, PartialEq, Eq)]
#[shrink_wrap(no_alloc)]
#[owned = "std"]
pub struct VersionControl<'i> {
    #[flag]
    branch: bool,
    #[flag]
    commit_short_id: bool,
    #[flag]
    commit_id: bool,
    pub dirty: bool,
    pub commit_id: Option<&'i str>,
    pub commit_short_id: Option<&'i str>,
    pub commit_timestamp: DateTime,
    pub branch: Option<&'i str>,
    pub tags: RefVec<'i, &'i str>,
}

// requires Borrow, which is a bit tricky to implement
// #[cfg(feature = "std")]
// impl ToOwned for BedrockBuildInfo<'_> {
//     type Owned = BedrockBuildInfoOwned;
//
//     fn to_owned(&self) -> Self::Owned {
//         BedrockBuildInfoOwned {
//         }
//     }
// }

#[cfg(feature = "std")]
impl BedrockBuildInfo<'_> {
    pub fn make_owned(&self) -> BedrockBuildInfoOwned {
        BedrockBuildInfoOwned {
            timestamp: self.timestamp,
            profile: self.profile,
            optimization_level: self.optimization_level,
            crate_info: self.crate_info.make_owned(),
            target_info: self.target_info.make_owned(),
            compiler_info: self.compiler_info.make_owned(),
            version_control: self.version_control.as_ref().map(|v| v.make_owned()),
        }
    }
}

impl CrateInfo<'_> {
    #[cfg(feature = "std")]
    pub fn make_owned(&self) -> CrateInfoOwned {
        CrateInfoOwned {
            name: self.name.to_owned(),
            version: self.version.make_owned(),
            authors: self
                .authors
                .iter()
                .map(|s| s.unwrap().to_string())
                .collect(),
            enabled_features: self
                .enabled_features
                .iter()
                .map(|s| s.unwrap().to_string())
                .collect(),
            dependencies: self
                .dependencies
                .iter()
                .map(|s| s.unwrap().make_owned())
                .collect(),
        }
    }
}

impl TargetInfo<'_> {
    #[cfg(feature = "std")]
    pub fn make_owned(&self) -> TargetInfoOwned {
        TargetInfoOwned {
            triple: self.triple.map(|t| t.to_string()),
            arch: self.arch.map(|a| a.to_string()),
        }
    }
}

impl CompilerInfo<'_> {
    #[cfg(feature = "std")]
    pub fn make_owned(&self) -> CompilerInfoOwned {
        CompilerInfoOwned {
            version: self.version.make_owned(),
            channel: self.channel,
            host_triple: self.host_triple.map(|t| t.to_string()),
            commit_date: self.commit_date,
            flip_link: self.flip_link,
        }
    }
}

impl VersionControl<'_> {
    #[cfg(feature = "std")]
    pub fn make_owned(&self) -> VersionControlOwned {
        VersionControlOwned {
            dirty: self.dirty,
            commit_id: self.commit_id.map(|id| id.to_string()),
            commit_short_id: self.commit_short_id.map(|id| id.to_string()),
            commit_timestamp: self.commit_timestamp,
            branch: self.branch.map(|s| s.to_string()),
            tags: self.tags.iter().map(|s| s.unwrap().to_string()).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;
    // use tracing_subscriber::layer::SubscriberExt;
    // use tracing_subscriber::util::SubscriberInitExt;
    // use tracing_subscriber::{EnvFilter, fmt};
    use ww_date_time::NaiveDate;

    #[test]
    fn v1_version_compatibility_not_broken() {
        // tracing_subscriber::registry()
        //     .with(fmt::layer())
        //     .with(EnvFilter::new("trace"))
        //     .init();
        let build_info = BedrockBuildInfo {
            timestamp: DateTime::from_ymd_hms_utc_opt(2025, 7, 13, 16, 20, 0, 0).unwrap(),
            profile: Profile::Release,
            optimization_level: OptimizationLevel::O2,
            crate_info: CrateInfo {
                name: "awesome",
                version: Version::new(0, 1, 2),
                authors: RefVec::new(),
                enabled_features: RefVec::new_str_slice(&["f_a", "f_b"]),
                dependencies: RefVec::new(),
            },
            target_info: TargetInfo {
                triple: Some("xyz"),
                arch: Some("arm"),
            },
            compiler_info: CompilerInfo {
                version: Version::new(1, 87, 0),
                channel: CompilerChannel::Nightly,
                host_triple: None,
                commit_date: Some(NaiveDate::from_ymd_opt(2025, 5, 5).unwrap()),
            },
            version_control: Some(VersionControl {
                dirty: true,
                commit_id: None,
                commit_short_id: Some("abc"),
                commit_timestamp: DateTime::from_ymd_hms_utc_opt(2025, 7, 14, 16, 20, 0, 0)
                    .unwrap(),
                branch: None,
                tags: RefVec::new(),
            }),
        };
        let mut buf = [0u8; 256];
        let mut wr = BufWriter::new(&mut buf);
        build_info.ser_shrink_wrap(&mut wr).unwrap();
        let bytes = wr.finish_and_take().unwrap();
        // println!("{}: {bytes:02X?}", bytes.len());
        assert_eq!(
            bytes,
            hex!(
                "076B96C0 14" // DateTime profile,optimization,version_control_flag
                "617765736F6D65 0120 665F61 665F62 033207" // CrateInfo: "awesome" 0.1.2 "f_a" "f_b" lengths: 7, 0, 2, 3, 3, 0
                "C0 78797A 61726D 33" // TargetInfo: flags "xyz" "arm" lengths: 3, 3
                "1FA100A0 5280" // CompilerInfo: Version: 1,87,0 channel host_triple Some(Date) length of Version=2
                "50 616263 077396C0 03" // VersionControl: flags "abc" DateTime lengths: 3, 0
                "0 1 9 6 09 2A" // lengths from the back: 18, 8, 6, 9
            )
        );
        assert_eq!(bytes.len(), 50);
        let mut rd = BufReader::new(&bytes[..]);
        let build_info_des = BedrockBuildInfo::des_shrink_wrap(&mut rd).unwrap();
        assert_eq!(build_info, build_info_des);

        // check that owned representation matches as well
        let mut owned = build_info.make_owned();
        let mut buf = [0u8; 256];
        let mut wr = BufWriter::new(&mut buf);
        owned.ser_shrink_wrap(&mut wr).unwrap();
        let bytes_owned = wr.finish_and_take().unwrap();
        assert_eq!(bytes, bytes_owned);

        // check multiple dependencies
        owned.crate_info.dependencies.push(CrateInfoOwned {
            name: "dep1".to_string(),
            version: VersionOwned::new(0, 1, 2),
            authors: vec!["a1".into(), "a2".into()],
            enabled_features: vec!["f1".into(), "f2".into(), "f3".into()],
            dependencies: vec![],
        });
        owned.crate_info.dependencies.push(CrateInfoOwned {
            name: "dep2_".to_string(),
            version: VersionOwned::new(4, 5, 6),
            authors: vec!["a3".into(), "a4".into()],
            enabled_features: vec!["f4".into(), "f5".into(), "f6".into()],
            dependencies: vec![],
        });
        let mut buf = [0u8; 256];
        let mut wr = BufWriter::new(&mut buf);
        owned.ser_shrink_wrap(&mut wr).unwrap();
        let bytes_owned_multi_dep = wr.finish_and_take().unwrap();
        // println!(
        //     "{}: {bytes_owned_multi_dep:02X?}",
        //     bytes_owned_multi_dep.len()
        // );
        assert_eq!(
            bytes_owned_multi_dep,
            hex!(
                "076B96C0 14"
                "617765736F6D65 0120 665F61 665F62" // awesome 0.1.2 [] [f_a, f_b]
                "64657031 0120 6131 6132 6631 6632 6633 0022232224" // dep1 0.1.2 [a1, a2] [f1, f2, f3] 0 0 2 2 2 3 2 2 2 4
                "646570325F 4560 6133 6134 6634 6635 6636 0022232225" // dep2 4.5.6 [a3, a4] [f4, f5, f6] 0 0 2 2 2 3 2 2 2 5
                "6A 5A 2 3 3 2 0 7" // 22 21 2 3 3 2 0 7
                "C0 78797A 61726D 33"
                "1FA100A0 5280"
                "50 616263 077396C0 03"
                "0 1 9 6 09 7F"
            )
        );
        assert_eq!(bytes_owned_multi_dep.len(), 95);

        let mut rd = BufReader::new(&bytes_owned_multi_dep);
        let build_info_des = BedrockBuildInfoOwned::des_shrink_wrap(&mut rd).unwrap();
        assert_eq!(build_info_des, owned);
        // println!("{build_info_des:#?}");
    }

    #[test]
    fn v1_read() {
        // tracing_subscriber::registry()
        //     .with(fmt::layer())
        //     .with(EnvFilter::new("trace"))
        //     .init();

        pub const BUILD_INFO_FULL: [u8; 1022] = hex!(
        "062239DC5C623139335F64363030706C75735F667701006E75636C656F61737369676E2D7265736F7572636573041000000A626564726F636B5F6275696C6401"
        "000000596275696C642D696E666F2D6275696C6400850064656661756C74676974676974320437300A6366672D696610000006636F727465782D6D0770637269"
        "746963616C2D73656374696F6E637269746963616C2D73656374696F6E2D73696E676C652D636F7265696E6C696E652D61736D00294B0A3009636F727465782D"
        "6D2D727407506465766963650610396465666D74101069705F696E5F636F72650291056465666D742D7274741000000019656D62617373792D6578656375746F"
        "7207005F61726368617263682D636F727465782D6D6465666D746578656375746F722D696E746572727570746578656375746F722D7468726561646E69676874"
        "6C7907792A5595600A656D62617373792D667574757265730110000079656D62617373792D6E657406006465666D74646863707634646E736D656469756D2D65"
        "746865726E657470726F746F2D6970763470726F746F2D69707636746370756470003329297936509039656D62617373792D73746D333202005F74696D652D64"
        "726976657264656661756C746465666D7465787469727473746D3332683734337A6974696D6574696D652D6472697665722D74696D32756E737461626C652D70"
        "616300490A43924574919059656D62617373792D73796E6306206465666D74051049656D62617373792D74696D6504006465666D746465666D742D74696D6573"
        "74616D702D757074696D657469636B2D687A2D33325F3736380696A53049656D6265646465642D776562736F636B6574091300002A68747470617273651A1100"
        "000970616E69632D70726F626510006465666D746465666D742D6572726F727072696E742D6465666D740393953039706173746510F100000572616D702D6D61"
        "6B657202006C69626D04102972616E645F636F726506400000197374617469635F63656C6C210000003973746D33322D6D657461706163820000636F72746578"
        "2D6D2D727464656661756C746D65746164617461706163727473746D3332683734337A6903923097396059776972655F77656176657204006368726F6E6F6465"
        "6661756C746465666D7473746474726163696E672D657874656E646564000A357650390F1890A693A295D597A4F6A2C95994A0B9694A6A689295C2A5A7A6100A"
        "C07468756D627637656D2D6E6F6E652D65616269686661726D035A1FA1086E696768746C7918616172636836342D6170706C652D64617277696E802D84A7F030"
        "61663930613037636364633165316630643035376537303966663464356566306565623035366230616639306130059486486D61696E00470D3F3C3B3FD9"
        );

        let mut rd = BufReader::new(&BUILD_INFO_FULL[..]);
        let _build_info = BedrockBuildInfo::des_shrink_wrap(&mut rd).unwrap();
        println!("{:#?}", _build_info);
    }
}
