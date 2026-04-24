#[cfg(test)]
use std::sync::{Mutex, MutexGuard};

#[cfg(test)]
static ENV_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().expect("env lock should not be poisoned")
}
