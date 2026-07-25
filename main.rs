//! CPU Information Tool
//!
//! Uses the raw x86 `CPUID` instruction via inline assembly (`core::arch::asm!`)
//! to query the CPU vendor string, brand string, and supported instruction
//! set extensions (SSE family, AVX family, BMI, etc).
//!
//! Note: `ebx`/`rbx` is used internally by LLVM for position-independent code,
//! so it can't be freely clobbered. We save/restore it by hand inside the
//! asm block rather than declaring it as an output operand.

use std::arch::asm;

/// Result of a single CPUID leaf/subleaf query.
#[derive(Clone, Copy, Default)]
struct CpuidResult {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
}

/// Executes `CPUID` for the given `leaf` (eax) and `subleaf` (ecx).
///
/// # Safety
/// CPUID is available on every x86_64 CPU (it's part of the baseline ISA),
/// so this is safe to call unconditionally on this target.
fn cpuid(leaf: u32, subleaf: u32) -> CpuidResult {
    let eax_out: u32;
    let ebx_out: u32;
    let ecx_out: u32;
    let edx_out: u32;

    unsafe {
        asm!(
            // rbx is reserved by LLVM's codegen (used for the GOT/PIC base),
            // so we stash it in a scratch register, run cpuid, then restore it.
            "mov {tmp}, rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "mov rbx, {tmp}",
            tmp = out(reg) _,
            ebx_out = out(reg) ebx_out,
            inout("eax") leaf => eax_out,
            inout("ecx") subleaf => ecx_out,
            out("edx") edx_out,
        );
    }

    CpuidResult {
        eax: eax_out,
        ebx: ebx_out,
        ecx: ecx_out,
        edx: edx_out,
    }
}

/// Converts four register dwords (as they come out of CPUID) into an ASCII string.
fn regs_to_ascii(regs: &[u32]) -> String {
    let mut bytes = Vec::with_capacity(regs.len() * 4);
    for r in regs {
        bytes.extend_from_slice(&r.to_le_bytes());
    }
    String::from_utf8_lossy(&bytes)
        .trim_end_matches('\0')
        .trim()
        .to_string()
}

/// Reads the 12-character vendor ID string from CPUID leaf 0.
fn get_vendor_string() -> String {
    let r = cpuid(0, 0);
    // Vendor string is packed as EBX, EDX, ECX (in that order).
    regs_to_ascii(&[r.ebx, r.edx, r.ecx])
}

/// Reads the 48-character processor brand string from the extended
/// CPUID leaves 0x80000002-0x80000004, if the CPU supports them.
fn get_brand_string() -> Option<String> {
    let max_ext = cpuid(0x8000_0000, 0).eax;
    if max_ext < 0x8000_0004 {
        return None;
    }
    let mut regs = Vec::with_capacity(12);
    for leaf in 0x8000_0002u32..=0x8000_0004u32 {
        let r = cpuid(leaf, 0);
        regs.extend_from_slice(&[r.eax, r.ebx, r.ecx, r.edx]);
    }
    Some(regs_to_ascii(&regs))
}

/// A single feature flag: name, which CPUID leaf/register it comes from, and its bit.
struct Feature {
    name: &'static str,
    bit: u32,
}

fn bit_set(value: u32, bit: u32) -> bool {
    (value >> bit) & 1 == 1
}

fn main() {
    println!("=== CPU Information (via CPUID) ===\n");

    // --- Vendor string (leaf 0) ---
    let vendor = get_vendor_string();
    println!("Vendor ID     : {}", vendor);

    let max_std_leaf = cpuid(0, 0).eax;
    println!("Max std leaf  : 0x{:08X}", max_std_leaf);

    // --- Brand string (extended leaves) ---
    match get_brand_string() {
        Some(brand) => println!("Brand String  : {}", brand),
        None => println!("Brand String  : <not supported by this CPU>"),
    }
    println!();

    if max_std_leaf < 1 {
        println!("CPUID leaf 1 not supported; can't query feature flags.");
        return;
    }

    // --- Leaf 1: ECX / EDX feature flags ---
    let leaf1 = cpuid(1, 0);

    let edx_features: &[Feature] = &[
        Feature { name: "FPU", bit: 0 },
        Feature { name: "MMX", bit: 23 },
        Feature { name: "SSE", bit: 25 },
        Feature { name: "SSE2", bit: 26 },
        Feature { name: "HTT", bit: 28 },
    ];

    let ecx_features: &[Feature] = &[
        Feature { name: "SSE3", bit: 0 },
        Feature { name: "PCLMULQDQ", bit: 1 },
        Feature { name: "SSSE3", bit: 9 },
        Feature { name: "FMA", bit: 12 },
        Feature { name: "SSE4.1", bit: 19 },
        Feature { name: "SSE4.2", bit: 20 },
        Feature { name: "MOVBE", bit: 22 },
        Feature { name: "POPCNT", bit: 23 },
        Feature { name: "AES-NI", bit: 25 },
        Feature { name: "XSAVE", bit: 26 },
        Feature { name: "OSXSAVE", bit: 27 },
        Feature { name: "AVX", bit: 28 },
        Feature { name: "F16C", bit: 29 },
        Feature { name: "RDRAND", bit: 30 },
    ];

    println!("--- Instruction Set Extensions ---");
    print_feature_row(edx_features, leaf1.edx);
    print_feature_row(ecx_features, leaf1.ecx);

    // --- Leaf 7, subleaf 0: extended features (AVX2, AVX512, BMI, etc) ---
    if max_std_leaf >= 7 {
        let leaf7 = cpuid(7, 0);

        let ebx_features: &[Feature] = &[
            Feature { name: "FSGSBASE", bit: 0 },
            Feature { name: "BMI1", bit: 3 },
            Feature { name: "AVX2", bit: 5 },
            Feature { name: "BMI2", bit: 8 },
            Feature { name: "RDSEED", bit: 18 },
            Feature { name: "ADX", bit: 19 },
            Feature { name: "SHA", bit: 29 },
            Feature { name: "AVX512F", bit: 16 },
            Feature { name: "AVX512DQ", bit: 17 },
            Feature { name: "AVX512CD", bit: 28 },
            Feature { name: "AVX512BW", bit: 30 },
            Feature { name: "AVX512VL", bit: 31 },
        ];

        let ecx7_features: &[Feature] = &[
            Feature { name: "AVX512VBMI", bit: 1 },
            Feature { name: "GFNI", bit: 8 },
            Feature { name: "VAES", bit: 9 },
        ];

        print_feature_row(ebx_features, leaf7.ebx);
        print_feature_row(ecx7_features, leaf7.ecx);
    }

    println!();
}

/// Prints one line per feature list, in "NAME: yes/no" form, only if supported flags exist.
fn print_feature_row(features: &[Feature], reg_value: u32) {
    for f in features {
        let supported = bit_set(reg_value, f.bit);
        println!(
            "  {:<12} : {}",
            f.name,
            if supported { "yes" } else { "no" }
        );
    }
}
