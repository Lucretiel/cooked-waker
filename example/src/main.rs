use cooked_waker::{IntoWaker, Wake, WakeRef};
use std::{sync::Arc, task::Waker};

#[derive(Debug, Clone, IntoWaker)]
struct CustomWaker {
    id: i128,
}

impl CustomWaker {
    fn new(id: i128) -> Self {
        Self { id }
    }
}

impl WakeRef for CustomWaker {
    fn wake_by_ref(&self) {
        println!("wake by ref: {}", self.id);
    }
}

impl Wake for CustomWaker {
    fn wake(self) {
        println!("wake by value: {}", self.id);
    }
}

impl Drop for CustomWaker {
    fn drop(&mut self) {
        println!("dropping waker: {}", self.id);
    }
}

#[derive(Debug, Clone, WakeRef, Wake, IntoWaker)]
struct SharedWaker {
    waker: Arc<CustomWaker>,
}

fn main() {
    println!("Hello, world!");
    let waker = CustomWaker::new(11);
    let waker: Waker = waker.into_waker();

    println!("Waker: {:?}", waker);
    waker.wake_by_ref();
    waker.wake();
}
