use std::marker::PhantomData;
use std::task::{RawWaker, RawWakerVTable, Waker as StdWaker};
use stowaway::{self, Stowaway};

pub trait RefWaker: Clone {
    fn wake_by_ref(&self);
}

pub trait Waker: RefWaker + Sized {
    fn wake(self) {
        self.wake_by_ref();
    }

    fn get_vtable() -> &'static RawWakerVTable {
        static VTABLE: RawWakerVTable = RawWakerVTable::new(
            // clone
            vtable_clone_function::<Self>,
            vtable_wake_function::<Self>,
            vtable_wake_by_ref_function::<Self>,
            vtable_drop_function::<Self>,
        );
    }
}

unsafe fn vtable_clone_function<T>(raw: *const ()) -> RawWaker {
    let raw = raw as *mut ();
    let waker: &T = unsafe { stowaway::ref_from_stowed(&raw) };
    let cloned: T = waker.clone();
    let stowed: Stowaway<T> = Stowaway::new(cloned);
    let new_raw: *mut () = Stowaway::into_raw(stowed);
    RawWaker::new(new_raw, get_vtable::<T>())
}

unsafe fn vtable_wake_function<T>(raw: *const ()) {
    let raw = raw as *mut ();
    let waker: Stowaway<T> = unsafe { Stowaway::from_raw(raw) };
    waker.wake();
}

unsafe fn vtable_wake_by_ref_function<T>(raw: *const ()) {
    let raw = raw as *mut ();
    let waker: &T = unsafe { stowaway::ref_from_stowed(&raw) };
    waker.wake_by_ref();
}

unsafe fn vtable_drop_function<T>(raw: *const ()) {
    let raw = raw as *mut ();
    let _waker: Stowaway<T> = unsafe { Stowaway::from_raw(raw) };
}

#[inline]
const fn get_vtable<T: Waker>() -> &'static RawWakerVTable {
    let x: WakerVtable<T> = WakerVtable;
    static VTABLE: RawWakerVTable = RawWakerVTable::new(
        vtable_clone_function::<T>,
        vtable_wake_function::<T>,
        vtable_wake_by_ref_function::<T>,
        vtable_drop_function::<T>,
    );

    &VTABLE
}

#[inline]
fn make_raw_waker<T: Waker>(waker: T) -> RawWaker {
    let stowed: Stowaway<T> = Stowaway::new(waker);
    let new_raw: *mut () = Stowaway::into_raw(stowed);
    RawWaker::new(new_raw, get_vtable::<T>())
}

pub fn make_std_waker<T: Waker>(waker: T) -> StdWaker {
    let raw_waker = make_raw_waker(waker);
    unsafe { Waker::from_raw(raw_waker) }
}
