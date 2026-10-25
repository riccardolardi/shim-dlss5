//! Does the user's DLSS 5 model suit their GPU?
//!
//! NVIDIA's signed `nvngx_dlssnr.dll` (310.8) creates the neural feature on
//! RTX 50 only; on RTX 20/30/40 DLSS still runs but the neural pass does not
//! (`feature 18 create failed with 0xbad00001`). Those cards need ShortFuse's
//! modified compatibility build. Both builds are identified by the SHA-256s
//! that wilsjo2's OptiScaler DLSS-NR fork documents in `INSTALL-DLSSNR.md`.
//! NVIDIA ships new signed builds (the faster October 2026 model), so any
//! file with an intact NVIDIA signature also counts as NVIDIA's model.
//! Other files are not judged: a custom runtime may still work.

use serde::Serialize;
use ts_rs::TS;

use crate::platform::Signature;

/// Leaf certificate name on NVIDIA's own binaries.
pub const NVIDIA_SIGNER: &str = "NVIDIA Corporation";

/// NVIDIA's signed 310.8 model. Runs the neural pass on RTX 50.
pub const NVIDIA_SIGNED_SHA256: &str =
    "e16bcf15e16e13f527491cdf7845b2fe6521a738d8f7c9c721866a8496e1fc8e";
/// ShortFuse's compatibility build. RTX 20/30/40, keeps the RTX 50 path.
pub const COMPAT_SHA256: &str = "e67dee209320cdafe0e93e45675d7aa34323a53acc57a72b2e40a181581c989a";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GpuFamily {
    Rtx50,
    Rtx40,
    Rtx30,
    Rtx20,
    /// An NVIDIA card we can't place (workstation RTX, GTX, unusual name).
    OtherNvidia,
    NotNvidia,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ModelBuild {
    NvidiaSigned,
    Compat,
    Unknown,
}

/// What the UI should say. `None` from [`model_fit`] means "nothing to add".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ModelFit {
    /// A known build that runs the neural pass on this GPU.
    Fits,
    /// RTX 20/30/40 with NVIDIA's signed model, or with no model chosen yet.
    NeedsCompatBuild,
    /// A file that is neither known build. It may work; we can't tell.
    UnknownBuild,
    /// Not an NVIDIA GPU: the neural pass cannot run.
    NoNvidiaGpu,
}

/// GPU family from the adapter name, e.g. "NVIDIA GeForce RTX 4070 Laptop GPU".
pub fn gpu_family(name: &str) -> GpuFamily {
    let lower = name.to_lowercase();
    if !lower.contains("nvidia") && !lower.contains("geforce") {
        return GpuFamily::NotNvidia;
    }
    let Some(after) = lower.split("rtx").nth(1) else {
        return GpuFamily::OtherNvidia;
    };
    let digits: String = after
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    // Consumer cards are four digits (2060, 4090, 5070). Workstation names
    // ("RTX 6000 Ada", "RTX A4000") are not placed.
    if digits.len() != 4 || lower.contains(" ada") || lower.contains("blackwell") {
        return GpuFamily::OtherNvidia;
    }
    match &digits[..2] {
        "50" => GpuFamily::Rtx50,
        "40" => GpuFamily::Rtx40,
        "30" => GpuFamily::Rtx30,
        "20" => GpuFamily::Rtx20,
        _ => GpuFamily::OtherNvidia,
    }
}

/// An intact Authenticode signature by NVIDIA: the file is as NVIDIA built it.
pub fn nvidia_signed(signature: &Signature) -> bool {
    matches!(signature, Signature::Valid { signer } if signer.eq_ignore_ascii_case(NVIDIA_SIGNER))
}

pub fn model_build(sha256: &str, signature: &Signature) -> ModelBuild {
    let h = sha256.to_lowercase();
    if h == NVIDIA_SIGNED_SHA256 || nvidia_signed(signature) {
        ModelBuild::NvidiaSigned
    } else if h == COMPAT_SHA256 {
        ModelBuild::Compat
    } else {
        ModelBuild::Unknown
    }
}

/// The note for a GPU (if detected) and a model (if chosen).
pub fn model_fit(gpu: Option<GpuFamily>, model: Option<ModelBuild>) -> Option<ModelFit> {
    use GpuFamily as G;
    use ModelBuild as M;
    let gpu = gpu?;
    match (gpu, model) {
        (G::NotNvidia, _) => Some(ModelFit::NoNvidiaGpu),
        (_, Some(M::Unknown)) => Some(ModelFit::UnknownBuild),
        (G::Rtx50, Some(_)) => Some(ModelFit::Fits),
        (G::Rtx40 | G::Rtx30 | G::Rtx20, Some(M::Compat)) => Some(ModelFit::Fits),
        (G::Rtx40 | G::Rtx30 | G::Rtx20, Some(M::NvidiaSigned) | None) => {
            Some(ModelFit::NeedsCompatBuild)
        }
        (G::Rtx50 | G::OtherNvidia, None) => None,
        (G::OtherNvidia, Some(_)) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_names_map_to_families() {
        let cases = [
            ("NVIDIA GeForce RTX 5070", GpuFamily::Rtx50),
            ("NVIDIA GeForce RTX 5090 D", GpuFamily::Rtx50),
            ("NVIDIA GeForce RTX 4070 Laptop GPU", GpuFamily::Rtx40),
            ("NVIDIA GeForce RTX 3060", GpuFamily::Rtx30),
            ("NVIDIA GeForce RTX 2080 SUPER", GpuFamily::Rtx20),
            ("NVIDIA RTX 6000 Ada Generation", GpuFamily::OtherNvidia),
            ("NVIDIA RTX A4000", GpuFamily::OtherNvidia),
            ("NVIDIA GeForce GTX 1080 Ti", GpuFamily::OtherNvidia),
            ("AMD Radeon RX 9070 XT", GpuFamily::NotNvidia),
            ("Intel(R) Arc(TM) B580 Graphics", GpuFamily::NotNvidia),
        ];
        for (name, want) in cases {
            assert_eq!(gpu_family(name), want, "{name}");
        }
    }

    #[test]
    fn known_hashes_are_recognised_in_any_case() {
        let none = Signature::Unsigned;
        assert_eq!(
            model_build(&NVIDIA_SIGNED_SHA256.to_uppercase(), &none),
            ModelBuild::NvidiaSigned
        );
        assert_eq!(model_build(COMPAT_SHA256, &none), ModelBuild::Compat);
        assert_eq!(model_build(&"0".repeat(64), &none), ModelBuild::Unknown);
    }

    #[test]
    fn a_new_build_counts_as_nvidias_only_with_an_intact_nvidia_signature() {
        let new = "1".repeat(64);
        let signed = |signer: &str| Signature::Valid {
            signer: signer.into(),
        };
        assert_eq!(
            model_build(&new, &signed("NVIDIA Corporation")),
            ModelBuild::NvidiaSigned
        );
        for other in [
            signed("Someone Else"),
            Signature::HashMismatch {
                signer: "NVIDIA Corporation".into(),
            },
            Signature::Unsigned,
            Signature::Unknown("no Windows".into()),
        ] {
            assert_eq!(model_build(&new, &other), ModelBuild::Unknown, "{other:?}");
        }
    }

    #[test]
    fn rtx50_fits_either_known_build_and_needs_no_note_without_a_model() {
        let g = Some(GpuFamily::Rtx50);
        assert_eq!(
            model_fit(g, Some(ModelBuild::NvidiaSigned)),
            Some(ModelFit::Fits)
        );
        assert_eq!(model_fit(g, Some(ModelBuild::Compat)), Some(ModelFit::Fits));
        assert_eq!(model_fit(g, None), None);
    }

    #[test]
    fn older_rtx_needs_the_compat_build() {
        for g in [GpuFamily::Rtx40, GpuFamily::Rtx30, GpuFamily::Rtx20] {
            assert_eq!(
                model_fit(Some(g), Some(ModelBuild::NvidiaSigned)),
                Some(ModelFit::NeedsCompatBuild),
                "{g:?}"
            );
            assert_eq!(model_fit(Some(g), None), Some(ModelFit::NeedsCompatBuild));
            assert_eq!(
                model_fit(Some(g), Some(ModelBuild::Compat)),
                Some(ModelFit::Fits)
            );
        }
    }

    #[test]
    fn unknown_builds_and_gpus_are_not_judged() {
        assert_eq!(
            model_fit(Some(GpuFamily::Rtx40), Some(ModelBuild::Unknown)),
            Some(ModelFit::UnknownBuild)
        );
        assert_eq!(model_fit(None, Some(ModelBuild::NvidiaSigned)), None);
        assert_eq!(
            model_fit(Some(GpuFamily::OtherNvidia), Some(ModelBuild::Compat)),
            None
        );
        assert_eq!(
            model_fit(Some(GpuFamily::NotNvidia), None),
            Some(ModelFit::NoNvidiaGpu)
        );
    }
}
