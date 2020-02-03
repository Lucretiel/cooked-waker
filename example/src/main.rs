use std::thread::Thread;

use cooked_waker::{IntoWaker, RefWake, Wake};
use std::thread;

#[derive(Debug, Clone, IntoWaker)]
struct ThreadWaker {
    thread: Thread,
}

impl ThreadWaker {
    fn from_current() -> Self {
        Self {
            thread: thread::current(),
        }
    }
}

impl RefWake for ThreadWaker {
    fn wake_by_ref(&self) {
        self.thread.unpark();
    }
}

impl Wake for ThreadWaker {}

fn main() {
    println!("Hello, world!");

    let waker = ThreadWaker::from_current();
    let waker = waker.into_waker();

    eprintln!("{:?}", &waker);
}
