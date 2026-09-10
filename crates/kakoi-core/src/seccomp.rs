//! The seccomp filter of specification section 11: a classic BPF program with the three
//! rules only — kill the process on a foreign architecture, kill it on an x32 system
//! call number, and fail `ioctl(TIOCSTI)` with `EPERM`. Everything else passes.

use std::mem::offset_of;

use libc::{seccomp_data, SECCOMP_RET_ALLOW, SECCOMP_RET_ERRNO, SECCOMP_RET_KILL_PROCESS};

/// One instruction as `struct sock_filter` lays it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction {
    pub code: u16,
    pub jt: u8,
    pub jf: u8,
    pub k: u32,
}

const fn load(offset: usize) -> Instruction {
    Instruction {
        code: (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16,
        jt: 0,
        jf: 0,
        k: offset as u32,
    }
}

const fn jump_if_equal(k: u32, jt: u8, jf: u8) -> Instruction {
    Instruction {
        code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
        jt,
        jf,
        k,
    }
}

const fn jump_if_any_bit(k: u32, jt: u8, jf: u8) -> Instruction {
    Instruction {
        code: (libc::BPF_JMP | libc::BPF_JSET | libc::BPF_K) as u16,
        jt,
        jf,
        k,
    }
}

const fn ret(k: u32) -> Instruction {
    Instruction {
        code: (libc::BPF_RET | libc::BPF_K) as u16,
        jt: 0,
        jf: 0,
        k,
    }
}

/// `AUDIT_ARCH_X86_64` of `linux/audit.h` (`EM_X86_64 | __AUDIT_ARCH_64BIT |
/// __AUDIT_ARCH_LE`): not in the `libc` crate, so written out.
const AUDIT_ARCH_X86_64: u32 = 0xC000_003E;
/// `__X32_SYSCALL_BIT` of `asm/unistd.h`: not in the `libc` crate, so written out.
const X32_SYSCALL_BIT: u32 = 0x4000_0000;
/// The kernel takes the request as 32 bits, so the low word of the second argument is
/// compared: a caller cannot slip past with high bits set.
const ARGUMENT_1_LOW_WORD: usize = offset_of!(seccomp_data, args) + 8;

/// The filter. Jump offsets count the instructions to skip.
pub const FILTER: [Instruction; 11] = [
    load(offset_of!(seccomp_data, arch)),
    jump_if_equal(AUDIT_ARCH_X86_64, 1, 0),
    ret(SECCOMP_RET_KILL_PROCESS),
    load(offset_of!(seccomp_data, nr)),
    jump_if_any_bit(X32_SYSCALL_BIT, 0, 1),
    ret(SECCOMP_RET_KILL_PROCESS),
    jump_if_equal(libc::SYS_ioctl as u32, 0, 3),
    load(ARGUMENT_1_LOW_WORD),
    jump_if_equal(libc::TIOCSTI as u32, 0, 1),
    ret(SECCOMP_RET_ERRNO | libc::EPERM as u32),
    ret(SECCOMP_RET_ALLOW),
];

/// The filter as the bytes bwrap reads from the `--seccomp` descriptor.
pub fn filter_bytes() -> Vec<u8> {
    FILTER
        .iter()
        .flat_map(|instruction| {
            let mut bytes = Vec::with_capacity(8);
            bytes.extend_from_slice(&instruction.code.to_le_bytes());
            bytes.push(instruction.jt);
            bytes.push(instruction.jf);
            bytes.extend_from_slice(&instruction.k.to_le_bytes());
            bytes
        })
        .collect()
}
