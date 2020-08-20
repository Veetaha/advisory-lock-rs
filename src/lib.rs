use std::{
    fs::{File, OpenOptions},
    io,
    ops::{Deref, DerefMut},
    path::Path,
};

use thiserror::Error;

#[cfg(windows)]
mod windows;

#[cfg(unix)]
mod unix;

/// An enumeration of possible errors which can occur while trying to acquire a lock.
#[derive(Debug, Error)]
pub enum FileLockError {
    /// The file is already locked by other process.
    #[error("the file is already locked")]
    AlreadyLocked,
    /// The error occurred during I/O operations.
    #[error("I/O error: {0}")]
    IOError(#[from] io::Error),
}

/// An enumeration of types which represents how to acquire an advisory lock.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FileLockMode {
    /// Obtain an exclusive file lock.
    Exclusive,
    /// Obtain a shared file lock.
    Shared,
}

/// An advisory lock for files.
///
/// An advisory lock provides a mutual-exclusion mechanism among processes which explicitly
/// acquires and releases the lock. Processes that are not aware of the lock will ignore it.
///
/// `AdvisoryFileLock` provides following features:
/// - Blocking or non-blocking operations.
/// - Shared or exclusive modes.
/// - All operations are thread-safe.
///
/// ## Notes
///
/// `AdvisoryFileLock` has following limitations:
/// - Locks are allowed only on files, but not directories.
pub struct AdvisoryFileLock {
    /// An underlying file.
    file: File,
    locked: bool,
    /// A file lock mode, shared or exclusive.
    file_lock_mode: FileLockMode,
}

impl AdvisoryFileLock {
    /// Create a new `FileLock`.
    pub fn new<P: AsRef<Path>>(
        path: P,
        file_lock_mode: FileLockMode,
    ) -> Result<Self, FileLockError> {
        let is_exclusive = file_lock_mode == FileLockMode::Exclusive;
        let file = OpenOptions::new()
            .read(true)
            .create(is_exclusive)
            .write(is_exclusive)
            .open(path)?;

        Ok(AdvisoryFileLock {
            file,
            locked: false,
            file_lock_mode,
        })
    }

    /// Return `true` if the advisory lock is acquired by shared mode.
    pub fn is_shared(&self) -> bool {
        self.file_lock_mode == FileLockMode::Shared
    }

    /// Return `true` if the advisory lock is acquired by exclusive mode.
    pub fn is_exclusive(&self) -> bool {
        self.file_lock_mode == FileLockMode::Exclusive
    }

    /// Acquire the advisory file lock.
    ///
    /// `lock` is blocking; it will block the current thread until it succeeds or errors.
    pub fn lock(&mut self) -> Result<(), FileLockError> {
        self.lock_impl()?;
        self.locked = true;
        Ok(())
    }

    /// Try to acquire the advisory file lock.
    ///
    /// `try_lock` returns immediately.
    pub fn try_lock(&mut self) -> Result<(), FileLockError> {
        self.try_lock_impl()?;
        self.locked = true;
        Ok(())
    }

    /// Unlock this advisory file lock.
    pub fn unlock(&mut self) -> Result<(), FileLockError> {
        self.unlock_impl()?;
        self.locked = false;
        Ok(())
    }
}

impl Drop for AdvisoryFileLock {
    fn drop(&mut self) {
        if !self.locked {
            return;
        }
        if let Err(err) = self.unlock() {
            log::error!(
                "[AdvisoryFileLock] unlock_file failed during dropping: {}",
                err
            );
        }
    }
}

impl Deref for AdvisoryFileLock {
    type Target = File;

    fn deref(&self) -> &Self::Target {
        &self.file
    }
}

impl DerefMut for AdvisoryFileLock {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.file
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn simple_shared_lock() {
        let mut test_file = temp_dir();
        test_file.push("shared_lock");
        File::create(&test_file).unwrap();
        {
            let mut f1 = AdvisoryFileLock::new(&test_file, FileLockMode::Shared).unwrap();
            f1.lock().unwrap();
            let mut f2 = AdvisoryFileLock::new(&test_file, FileLockMode::Shared).unwrap();
            f2.lock().unwrap();
        }
        std::fs::remove_file(&test_file).unwrap();
    }

    #[test]
    fn simple_exclusive_lock() {
        let mut test_file = temp_dir();
        test_file.push("exclusive_lock");
        {
            let mut f1 = AdvisoryFileLock::new(&test_file, FileLockMode::Exclusive).unwrap();
            f1.lock().unwrap();
            let f2 = AdvisoryFileLock::new(&test_file, FileLockMode::Exclusive)
                .unwrap()
                .try_lock();
            assert!(f2.is_err());
        }
        std::fs::remove_file(&test_file).unwrap();
    }

    #[test]
    fn simple_shared_exclusive_lock() {
        let mut test_file = temp_dir();
        test_file.push("shared_exclusive_lock");
        File::create(&test_file).unwrap();
        {
            let mut f1 = AdvisoryFileLock::new(&test_file, FileLockMode::Shared).unwrap();
            f1.lock().unwrap();
            let mut f2 = AdvisoryFileLock::new(&test_file, FileLockMode::Exclusive).unwrap();
            assert!(f2.try_lock().is_err());
        }
        std::fs::remove_file(&test_file).unwrap();
    }

    #[test]
    fn simple_exclusive_shared_lock() {
        let mut test_file = temp_dir();
        test_file.push("exclusive_shared_lock");
        {
            let mut f1 = AdvisoryFileLock::new(&test_file, FileLockMode::Exclusive).unwrap();
            f1.lock().unwrap();
            let mut f2 = AdvisoryFileLock::new(&test_file, FileLockMode::Exclusive).unwrap();
            assert!(f2.try_lock().is_err());
        }
        std::fs::remove_file(&test_file).unwrap();
    }
}
