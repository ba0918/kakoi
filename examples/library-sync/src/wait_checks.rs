//! A descheduled executor's waker clone must not hold the product's result/event lock.
use std::future::Future;
use std::io::{Read, Write};
use std::sync::{mpsc, Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::Duration;

struct CloneGate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
static TABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_ref, drop_waker);
unsafe fn clone(data: *const ()) -> RawWaker {
    // SAFETY: every raw waker owns one Arc<CloneGate>; borrowing leaves its count intact.
    let gate = unsafe { &*data.cast::<CloneGate>() };
    gate.entered.send(()).unwrap();
    gate.release
        .lock()
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .expect("executor cloning held a result/event lock and blocked real completion");
    unsafe {
        Arc::increment_strong_count(data.cast::<CloneGate>());
    }
    RawWaker::new(data, &TABLE)
}
unsafe fn wake(data: *const ()) {
    // The result is polled directly after the registration race, not by this test waker.
    unsafe {
        drop(Arc::from_raw(data.cast::<CloneGate>()));
    }
}
unsafe fn wake_ref(_: *const ()) {}
unsafe fn drop_waker(data: *const ()) {
    unsafe {
        drop(Arc::from_raw(data.cast::<CloneGate>()));
    }
}

pub fn registration_race(completion: bool) {
    let mut running = kakoi_runtime::prepare(crate::cat_request())
        .unwrap()
        .spawn()
        .unwrap();
    let mut output = running.take_stdout().unwrap();
    let mut input = running.take_stdin().unwrap();
    let mut ready = [0; 5];
    output.read_exact(&mut ready).unwrap();
    assert_eq!(&ready, b"ready");
    let mut events = running.events();
    let (entered, entering) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let gate = Arc::new(CloneGate {
        entered,
        release: Mutex::new(released),
    });
    let raw = RawWaker::new(Arc::into_raw(gate).cast(), &TABLE);
    // SAFETY: TABLE keeps one Arc alive per raw waker and implements thread-safe callbacks.
    let waker = unsafe { Waker::from_raw(raw) };
    let mut context = Context::from_waker(&waker);
    std::thread::scope(|scope| {
        let product = &running;
        let producer = scope.spawn(move || {
            entering.recv_timeout(Duration::from_secs(10)).unwrap();
            input.write_all(b"registration race").unwrap();
            drop(input);
            // Completion is produced by the real command, init, worker and reaper.
            let outcome = product.wait();
            let _ = release.send(());
            outcome
        });
        if completion {
            let mut future = Box::pin(running.wait_async());
            let Poll::Ready(outcome) = future.as_mut().poll(&mut context) else {
                panic!("completed outcome was lost during registration")
            };
            assert!(Arc::ptr_eq(&outcome, &producer.join().unwrap()));
        } else {
            let mut future = Box::pin(events.recv_async());
            let Poll::Ready(kakoi_runtime::EventRead::Event(event)) =
                future.as_mut().poll(&mut context)
            else {
                panic!("unread event was lost during registration")
            };
            assert!(matches!(
                event.kind,
                kakoi_runtime::RunEventKind::Status(kakoi_runtime::RunStatus::Stopping)
            ));
            assert!(Arc::ptr_eq(&producer.join().unwrap(), &running.wait()));
        }
    });
    let mut bytes = Vec::new();
    output.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"registration race");
    assert_eq!(running.wait().main, kakoi_runtime::MainOutcome::Exited(0));
}
