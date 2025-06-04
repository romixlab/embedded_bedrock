use bedrock_build_info::{
    BedrockBuildInfoOwned, CompilerChannel, CompilerInfoOwned, CrateInfoOwned, OptimizationLevel,
    Profile, TargetInfoOwned, VersionControlOwned,
};
use build_info_common::BuildInfo;
use shrink_wrap::prelude::*;

pub fn shrink_wrap_build_info(info: BuildInfo) -> (Vec<u8>, Vec<u8>) {
    let mut info = BedrockBuildInfoOwned {
        timestamp: info.timestamp.into(),
        profile: profile(info.profile),
        optimization_level: optimization_level(info.optimization_level),
        crate_info: crate_info(info.crate_info),
        target_info: TargetInfoOwned {
            triple: Some(info.target.triple),
            arch: Some(info.target.cpu.arch),
        },
        compiler_info: CompilerInfoOwned {
            version: info.compiler.version.try_into().unwrap(),
            channel: compiler_info(info.compiler.channel),
            host_triple: Some(info.compiler.host_triple),
            commit_date: info.compiler.commit_date.map(|date| date.into()),
        },
        version_control: info.version_control.map(|v| {
            let build_info_common::VersionControl::Git(git) = v;
            VersionControlOwned {
                dirty: git.dirty,
                commit_id: Some(git.commit_id),
                commit_short_id: Some(git.commit_short_id),
                commit_timestamp: git.commit_timestamp.try_into().unwrap(),
                branch: git.branch,
                tags: git.tags,
            }
        }),
    };
    // remove nanoseconds for both
    info.timestamp.time.frac = None;

    if let Some(vc) = &mut info.version_control {
        vc.commit_timestamp.time.frac = None;
    }
    for dep in &mut info.crate_info.dependencies {
        dep.authors = Vec::new();
    }

    // let build_info_debug = format!("{:#?}", info);
    let mut buf = [0u8; 16_384];
    let mut wr = BufWriter::new(&mut buf);
    info.ser_shrink_wrap(&mut wr).unwrap();
    let info_full = wr.finish_and_take().unwrap().to_vec();

    info.crate_info.dependencies = Vec::new();
    if let Some(vc) = &mut info.version_control {
        vc.commit_id = None;
    }
    // let mut buf = [0u8; 16_384]; // TODO: remove
    let mut wr = BufWriter::new(&mut buf);
    info.ser_shrink_wrap(&mut wr).unwrap();
    let info_pruned = wr.finish_and_take().unwrap().to_vec();

    (info_full, info_pruned)
}

fn crate_info(info: build_info_common::CrateInfo) -> CrateInfoOwned {
    CrateInfoOwned {
        name: info.name,
        version: info.version.try_into().unwrap(),
        authors: info.authors,
        enabled_features: info.enabled_features,
        dependencies: info
            .dependencies
            .into_iter()
            .map(|dep| crate_info(dep))
            .collect(),
    }
}

fn profile(profile: String) -> Profile {
    match profile.as_str() {
        "release" => Profile::Release,
        "debug" => Profile::Debug,
        _ => panic!("Unrecognized profile: {}", profile),
    }
}

fn optimization_level(level: build_info_common::OptimizationLevel) -> OptimizationLevel {
    match level {
        build_info_common::OptimizationLevel::O0 => OptimizationLevel::O0,
        build_info_common::OptimizationLevel::O1 => OptimizationLevel::O1,
        build_info_common::OptimizationLevel::O2 => OptimizationLevel::O2,
        build_info_common::OptimizationLevel::O3 => OptimizationLevel::O3,
        build_info_common::OptimizationLevel::Os => OptimizationLevel::Os,
        build_info_common::OptimizationLevel::Oz => OptimizationLevel::Oz,
    }
}

fn compiler_info(channel: build_info_common::CompilerChannel) -> CompilerChannel {
    match channel {
        build_info_common::CompilerChannel::Dev => CompilerChannel::Dev,
        build_info_common::CompilerChannel::Nightly => CompilerChannel::Nightly,
        build_info_common::CompilerChannel::Beta => CompilerChannel::Beta,
        build_info_common::CompilerChannel::Stable => CompilerChannel::Stable,
    }
}
