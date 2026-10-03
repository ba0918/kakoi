// Linux x86_64 is the supported platform. Query the kernel representation so
// libc-specific sigaction layouts cannot mask differences between GNU and musl.
use std::collections::BTreeMap;

unsafe extern "C" {
    fn syscall(number: i64, ...) -> i64;
    fn sigaction(signal: i32, action: *const u8, previous: *mut u8) -> i32;
    fn signal(signal: i32, handler: usize) -> usize;
}

#[repr(C, align(16))]
struct LibcAction([u8; 256]);

extern "C" fn application_handler(_: i32) {}

pub fn install_application_state() {
    // Exercise preservation of a real caller handler and a nonempty mask.
    // These changes belong to the fixture, before its baseline, not the product.
    assert_ne!(
        unsafe { signal(10, application_handler as *const () as usize) },
        usize::MAX
    );
    let blocked = 1u64 << (12 - 1);
    assert_eq!(unsafe { syscall(14, 0i32, &blocked, 0usize, 8usize) }, 0);
}

#[derive(Debug, PartialEq, Eq)]
pub struct Signals {
    pub actions: BTreeMap<i32, [u64; 4]>,
    pub usable: u64,
    pub mask: u64,
}

impl Signals {
    pub fn capture() -> Self {
        let mut actions = BTreeMap::new();
        let mut usable = 0;
        for signal in 1..=64 {
            let mut libc_action = LibcAction([0; 256]);
            // SAFETY: the oversized aligned buffer accommodates either libc's
            // sigaction; a null input queries without changing any handler.
            if unsafe { sigaction(signal, std::ptr::null(), libc_action.0.as_mut_ptr()) } == 0 {
                usable |= 1 << (signal - 1);
                let mut action = [0u64; 4];
                // SAFETY: x86_64 rt_sigaction writes its four-word kernel ABI.
                assert_eq!(
                    unsafe { syscall(13, signal, 0usize, action.as_mut_ptr(), 8usize) },
                    0
                );
                actions.insert(signal, action);
            }
        }
        let mut mask = 0u64;
        // SAFETY: a null input queries the calling thread's complete kernel mask.
        assert_eq!(unsafe { syscall(14, 0i32, 0usize, &mut mask, 8usize) }, 0);
        Self {
            actions,
            usable,
            mask,
        }
    }

    pub fn assert_preserved(&self) {
        assert_eq!(
            &Self::capture(),
            self,
            "application handlers or caller mask changed"
        );
    }
}

pub fn raw_state() -> [u64; 3] {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    ["SigBlk:", "SigIgn:", "SigCgt:"].map(|name| {
        u64::from_str_radix(
            status
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .unwrap()
                .trim(),
            16,
        )
        .unwrap()
    })
}

pub fn report(before: [u64; 3], usable: u64) {
    let after = raw_state();
    println!(
        "signals {usable:016x} {:016x} {:016x} {:016x}",
        before[0] ^ after[0],
        before[1] ^ after[1],
        before[2] ^ after[2]
    );
}
