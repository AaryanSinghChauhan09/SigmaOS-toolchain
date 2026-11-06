//! SigmaOS-toolchain — cross-compilation toolchain configuration.
//!
//! Defines the build targets SigmaOS produces and the flags/linker
//! settings each requires:
//! - `x86_64-unknown-none` — bare-metal kernel (no libc, no OS)
//! - `x86_64-unknown-linux-gnu` — hosted userland tools
//! - `aarch64-unknown-none` — ARM64 bare-metal kernel
//!
//! Produces target-spec JSON compatible with `rustc --target` custom
//! specs, linker argument sets, and sysroot layout descriptions.
//! Pure data + validation; invoking rustc/linker is the caller's job
//! (mechanism/policy separation).

use std::fmt;

/// Errors building or querying toolchain configuration — fail-fast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolchainError {
    UnknownTarget(String),
    InvalidOptLevel(u8),
    EmptySysroot,
    ConflictingFlags(Vec<String>),
}

impl fmt::Display for ToolchainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolchainError::UnknownTarget(t) => write!(f, "unknown target: {t}"),
            ToolchainError::InvalidOptLevel(o) => write!(f, "invalid opt-level: {o}"),
            ToolchainError::EmptySysroot => write!(f, "sysroot path is empty"),
            ToolchainError::ConflictingFlags(fs) => write!(f, "conflicting flags: {}", fs.join(", ")),
        }
    }
}
impl std::error::Error for ToolchainError {}

/// Target architecture family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    AArch64,
    RiscV64,
}

/// Target environment: bare-metal kernel vs hosted userland.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Env {
    /// `-none`: no OS, no libc — kernel/firmware builds.
    None,
    /// `-linux-gnu`: hosted against SigmaOS userland ABI.
    SigmaGnu,
    /// `-linux-musl`: static hosted builds.
    SigmaMusl,
}

/// A build target: arch + env + optional CPU/features.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub arch: Arch,
    pub env: Env,
    /// LLVM target-cpu feature, e.g. "x86-64-v3".
    pub cpu: Option<String>,
}

impl Target {
    /// Rust target-triple string.
    pub fn triple(&self) -> String {
        let arch = match self.arch {
            Arch::X86_64 => "x86_64",
            Arch::AArch64 => "aarch64",
            Arch::RiscV64 => "riscv64",
        };
        let env = match self.env {
            Env::None => "unknown-none",
            Env::SigmaGnu => "unknown-linux-gnu",
            Env::SigmaMusl => "unknown-linux-musl",
        };
        format!("{arch}-{env}")
    }

    /// True for bare-metal targets (need linker script, no libc start files).
    pub fn is_bare_metal(&self) -> bool {
        self.env == Env::None
    }
}

/// Well-known SigmaOS targets.
pub mod targets {
    use super::{Arch, Env, Target};
    /// Default kernel target.
    pub const KERNEL_X86_64: Target = Target { arch: Arch::X86_64, env: Env::None, cpu: None };
    /// Userland tools target.
    pub const USERLAND_X86_64: Target = Target { arch: Arch::X86_64, env: Env::SigmaGnu, cpu: None };
    /// Static userland (musl-style, no dynamic loader needed).
    pub const USERLAND_STATIC: Target = Target { arch: Arch::X86_64, env: Env::SigmaMusl, cpu: None };
    /// AArch64 kernel.
    pub const KERNEL_AARCH64: Target = Target { arch: Arch::AArch64, env: Env::None, cpu: None };
}

/// Compiler/linker flag profile for a target.
#[derive(Debug, Clone)]
pub struct FlagProfile {
    /// `-C` flags passed to rustc.
    pub rustflags: Vec<String>,
    /// Linker (`ld`/`rust-lld`) arguments.
    pub linker_args: Vec<String>,
    /// Whether a custom linker script is required (bare-metal).
    pub needs_linker_script: bool,
    /// Suggested linker script name in `sigmaos-toolchain/scripts/`.
    pub linker_script: Option<&'static str>,
}

/// Compute the flag profile for a target — pure function, deterministic.
pub fn flag_profile(target: &Target, opt_level: u8) -> Result<FlagProfile, ToolchainError> {
    if opt_level > 3 {
        return Err(ToolchainError::InvalidOptLevel(opt_level));
    }
    let mut rustflags = vec![
        "-C".into(),
        format!("opt-level={opt_level}"),
        "-C".into(),
        "panic=abort".into(),
        "-C".into(),
        "relocation-model=static".into(),
    ];
    if let Some(cpu) = &target.cpu {
        rustflags.push("-C".into());
        rustflags.push(format!("target-cpu={cpu}"));
    }

    let (linker_args, needs_script, script) = if target.is_bare_metal() {
        (
            vec![
                "-nostdlib".into(),
                "-static".into(),
                "-no-pie".into(),
            ],
            true,
            Some(match target.arch {
                Arch::X86_64 => "kernel_x86_64.ld",
                Arch::AArch64 => "kernel_aarch64.ld",
                Arch::RiscV64 => "kernel_riscv64.ld",
            }),
        )
    } else {
        let mut args = vec!["-pie".into()];
        if target.env == Env::SigmaMusl {
            args.push("-static".into());
        }
        (args, false, None)
    };
    Ok(FlagProfile { rustflags, linker_args, needs_linker_script: needs_script, linker_script: script })
}

/// Generate a `rustc --target` custom JSON spec skeleton for a bare-metal
/// target. Fields mirror `rustc --print target-spec-json` output.
pub fn target_spec_json(target: &Target) -> String {
    let llvm_arch = match target.arch {
        Arch::X86_64 => "x86_64",
        Arch::AArch64 => "aarch64",
        Arch::RiscV64 => "riscv64",
    };
    let data_layout = match target.arch {
        Arch::X86_64 => "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128",
        Arch::AArch64 => "e-m:e-p270:32:32-p271:32:32-p272:64:64-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128-Fn32",
        Arch::RiscV64 => "e-m:e-p:64:64-i64:64-i128:128-n32:64-S128",
    };
    format!(
        r#"{{
  "llvm-target": "{llvm_arch}-unknown-none",
  "data-layout": "{data_layout}",
  "arch": "{}",
  "target-endian": "little",
  "target-pointer-width": "64",
  "target-c-int-width": "32",
  "os": "none",
  "env": "",
  "vendor": "unknown",
  "linker-flavor": "ld.lld",
  "linker": "rust-lld",
  "panic-strategy": "abort",
  "relocation-model": "static",
  "features": "+crt-static"
}}"#,
        match target.arch {
            Arch::X86_64 => "x86_64",
            Arch::AArch64 => "aarch64",
            Arch::RiscV64 => "riscv64",
        }
    )
}

/// Sysroot layout description — where staged artifacts land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SysrootLayout {
    /// Kernel images: `<sysroot>/boot/vmlinuz-<triple>`
    pub boot_dir: String,
    /// Initramfs images: `<sysroot>/boot/initramfs-<triple>.img`
    pub initramfs_pattern: String,
    /// Userland binaries: `<sysroot>/usr/bin/`
    pub userland_bin: String,
    /// Libraries: `<sysroot>/usr/lib/`
    pub userland_lib: String,
}

/// Standard SigmaOS sysroot layout for a target triple.
pub fn sysroot_layout(triple: &str) -> Result<SysrootLayout, ToolchainError> {
    if triple.trim().is_empty() {
        return Err(ToolchainError::EmptySysroot);
    }
    Ok(SysrootLayout {
        boot_dir: "/boot".into(),
        initramfs_pattern: format!("/boot/initramfs-{triple}.img"),
        userland_bin: "/usr/bin".into(),
        userland_lib: "/usr/lib".into(),
    })
}

// Back-compat shim: original API kept working.
pub struct ToolchainConfig;
impl ToolchainConfig {
    pub fn default_target_triple() -> &'static str {
        "x86_64-unknown-none"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triples_are_correct() {
        assert_eq!(targets::KERNEL_X86_64.triple(), "x86_64-unknown-none");
        assert_eq!(targets::USERLAND_X86_64.triple(), "x86_64-unknown-linux-gnu");
        assert_eq!(targets::KERNEL_AARCH64.triple(), "aarch64-unknown-none");
    }

    #[test]
    fn bare_metal_needs_linker_script() {
        let p = flag_profile(&targets::KERNEL_X86_64, 2).unwrap();
        assert!(p.needs_linker_script);
        assert_eq!(p.linker_script, Some("kernel_x86_64.ld"));
        assert!(p.linker_args.contains(&"-nostdlib".to_string()));
    }

    #[test]
    fn userland_is_pie() {
        let p = flag_profile(&targets::USERLAND_X86_64, 2).unwrap();
        assert!(!p.needs_linker_script);
        assert!(p.linker_args.contains(&"-pie".to_string()));
    }

    #[test]
    fn musl_static_adds_static_flag() {
        let p = flag_profile(&targets::USERLAND_STATIC, 2).unwrap();
        assert!(p.linker_args.contains(&"-static".to_string()));
    }

    #[test]
    fn opt_level_validated() {
        assert!(matches!(
            flag_profile(&targets::KERNEL_X86_64, 9),
            Err(ToolchainError::InvalidOptLevel(9))
        ));
    }

    #[test]
    fn target_spec_json_is_valid_shape() {
        let j = target_spec_json(&targets::KERNEL_X86_64);
        assert!(j.contains("\"llvm-target\": \"x86_64-unknown-none\""));
        assert!(j.contains("\"panic-strategy\": \"abort\""));
        assert!(j.contains("data-layout"));
    }

    #[test]
    fn sysroot_layout_paths() {
        let l = sysroot_layout("x86_64-unknown-none").unwrap();
        assert_eq!(l.initramfs_pattern, "/boot/initramfs-x86_64-unknown-none.img");
        assert_eq!(l.userland_bin, "/usr/bin");
    }
}
