///! Mutex implemented using AtomicBool
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

pub struct CasMutex<T: ?Sized> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for CasMutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for CasMutex<T> {}

pub struct CasMutexGuard<'a, T: ?Sized + 'a> {
    // funny underscores due to how Deref/DerefMut currently work (they
    // disregard field privacy).
    __lock: &'a CasMutex<T>,
}

impl<'a, T: ?Sized + 'a> CasMutexGuard<'a, T> {
    pub fn new(pool: &'a CasMutex<T>) -> CasMutexGuard<'a, T> {
        Self { __lock: pool }
    }
}

unsafe impl<'a, T: ?Sized + Sync> Sync for CasMutexGuard<'a, T> {}

impl<T> CasMutex<T> {
    pub fn new(t: T) -> CasMutex<T> {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(t),
        }
    }
}

impl<T: ?Sized> CasMutex<T> {
    pub fn lock(&self) -> CasMutexGuard<T> {
        while let Err(_) = self
            .locked
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        {}
        CasMutexGuard::new(self)
    }
}

impl<'mutex, T: ?Sized> Deref for CasMutexGuard<'mutex, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.__lock.data.get() }
    }
}

impl<'mutex, T: ?Sized> DerefMut for CasMutexGuard<'mutex, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.__lock.data.get() }
    }
}

impl<'a, T: ?Sized> Drop for CasMutexGuard<'a, T> {
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
        let m = CasMutex::new(());
        drop(m.lock());
        drop(m.lock());
    }

    #[test]
    fn lots_and_lots() {
        const J: u32 = 1000;
        const K: u32 = 3;

        let m = Arc::new(CasMutex::new(0));

        fn inc(m: &CasMutex<u32>) {
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
        let mutex: &CasMutex<[i32]> = &CasMutex::new([1, 2, 3]);
        {
            let b = &mut *mutex.lock();
            b[0] = 4;
            b[2] = 5;
        }
        let comp: &[i32] = &[4, 2, 5];
        assert_eq!(&*mutex.lock(), comp);
    }
}
