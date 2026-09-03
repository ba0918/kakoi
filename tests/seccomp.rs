use process_wrap::seccomp::filter_bytes;

/// One classic BPF instruction as `struct sock_filter` lays it out: `code`, `jt`, `jf`,
/// `k`, little-endian, eight bytes.
#[derive(Debug, PartialEq, Eq)]
struct Instruction {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}

fn decode(bytes: &[u8]) -> Vec<Instruction> {
    assert_eq!(bytes.len() % 8, 0, "{} bytes", bytes.len());
    bytes
        .chunks(8)
        .map(|chunk| Instruction {
            code: u16::from_le_bytes([chunk[0], chunk[1]]),
            jt: chunk[2],
            jf: chunk[3],
            k: u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
        })
        .collect()
}

const LD_W_ABS: u16 = (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16;
const JMP_JEQ_K: u16 = (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16;
const RET_K: u16 = (libc::BPF_RET | libc::BPF_K) as u16;
/// `AUDIT_ARCH_X86_64` of `linux/audit.h`: `EM_X86_64` with the 64-bit and
/// little-endian flags.
const AUDIT_ARCH_X86_64: u32 = 0xC000_003E;

#[test]
fn the_filter_begins_with_an_architecture_check_that_kills_the_process() {
    let program = decode(&filter_bytes());

    assert_eq!(
        program[0],
        Instruction {
            code: LD_W_ABS,
            jt: 0,
            jf: 0,
            k: std::mem::offset_of!(libc::seccomp_data, arch) as u32,
        }
    );
    assert_eq!(program[1].code, JMP_JEQ_K);
    assert_eq!(program[1].k, AUDIT_ARCH_X86_64);
    let on_mismatch = &program[2 + usize::from(program[1].jf)];
    assert_eq!(on_mismatch.code, RET_K);
    assert_eq!(on_mismatch.k, libc::SECCOMP_RET_KILL_PROCESS);
    let on_match = &program[2 + usize::from(program[1].jt)];
    assert_ne!(
        on_match.code, RET_K,
        "a matching architecture goes on to the next rule"
    );
}
