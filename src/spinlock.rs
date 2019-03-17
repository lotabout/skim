///! SpinLock implemented using AtomicBool
///! Just like Mutex except:
///!
///! 1. It uses CAS for locking, more efficient in low contention
///! 2. Use `.lock()` instead of `.lock().unwrap()` to retrieve the guard.
///! 3. It doesn't handle poison so data is still available on thread panic.
use std::cell::UnsafeCell;
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

pub struct SpinLock<T: ?Sized> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for SpinLock<T> {}
unsafe impl<T: ?Sized + Send> Sync for SpinLock<T> {}

pub struct SpinLockGuard<'a, T: ?Sized + 'a> {
    // funny underscores due to how Deref/DerefMut currently work (they
    // disregard field privacy).
    __lock: &'a SpinLock<T>,
}

impl<'a, T: ?Sized + 'a> SpinLockGuard<'a, T> {
    pub fn new(pool: &'a SpinLock<T>) -> SpinLockGuard<'a, T> {
        Self { __lock: pool }
    }
}

unsafe impl<'a, T: ?Sized + Sync> Sync for SpinLockGuard<'a, T> {}

impl<T> SpinLock<T> {
    pub fn new(t: T) -> SpinLock<T> {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(t),
        }
    }
}

impl<T: ?Sized> SpinLock<T> {
    pub fn lock(&self) -> SpinLockGuard<T> {
        while let Err(_) = self
            .locked
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        {}
        SpinLockGuard::new(self)
    }
}

impl<'mutex, T: ?Sized> Deref for SpinLockGuard<'mutex, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.__lock.data.get() }
    }
}

impl<'mutex, T: ?Sized> DerefMut for SpinLockGuard<'mutex, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.__lock.data.get() }
    }
}

impl<'a, T: ?Sized> Drop for SpinLockGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        while let Err(_) = self
            .__lock
            .locked
            .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
        {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::sync::Arc;
    use std::thread;

    #[derive(Eq, PartialEq, Debug)]
    struct NonCopy(i32);

    #[test]
    fn smoke() {
        let m = SpinLock::new(());
        drop(m.lock());
        drop(m.lock());
    }

    #[test]
    fn lots_and_lots() {
        const J: u32 = 1000;
        const K: u32 = 3;

        let m = Arc::new(SpinLock::new(0));

        fn inc(m: &SpinLock<u32>) {
            for _ in 0..J {
                *m.lock() += 1;
            }
        }

        let (tx, rx) = channel();
        for _ in 0..K {
            let tx2 = tx.clone();
            let m2 = m.clone();
            thread::spawn(move || {
                inc(&m2);
                tx2.send(()).unwrap();
            });
            let tx2 = tx.clone();
            let m2 = m.clone();
            thread::spawn(move || {
                inc(&m2);
                tx2.send(()).unwrap();
            });
        }

        drop(tx);
        for _ in 0..2 * K {
            rx.recv().unwrap();
        }
        assert_eq!(*m.lock(), J * K * 2);
    }

    #[test]
    fn test_mutex_unsized() {
        let mutex: &SpinLock<[i32]> = &SpinLock::new([1, 2, 3]);
        {
            let b = &mut *mutex.lock();
            b[0] = 4;
            b[2] = 5;
        }
        let comp: &[i32] = &[4, 2, 5];
        assert_eq!(&*mutex.lock(), comp);
    }
}
