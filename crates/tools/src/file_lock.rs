//! Per-file semaphore locking for concurrent edit protection
//!
//! Uses a static map of PathBuf -> Arc<Semaphore> to prevent concurrent edits
//! to the same file. The semaphore is acquired before edit and released when the
//! guard is dropped.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Global file lock map - lazily initialized
static FILE_LOCKS: std::sync::OnceLock<parking_lot::Mutex<HashMap<PathBuf, Arc<Semaphore>>>> =
    std::sync::OnceLock::new();

/// Get or create a semaphore for the given file path
fn get_file_semaphore(path: &Path) -> Arc<Semaphore> {
    let locks = FILE_LOCKS.get_or_init(|| parking_lot::Mutex::new(HashMap::new()));
    let mut guards = locks.lock();
    guards
        .entry(path.to_path_buf())
        .or_insert_with(|| Arc::new(Semaphore::new(1)))
        .clone()
}

/// RAII guard that holds a file lock permit
///
/// The permit is automatically released when this struct is dropped.
pub struct FileLockGuard {
    _permit: tokio::sync::SemaphorePermit<'static>,
}

impl FileLockGuard {
    /// Acquire a lock on the given file path, waiting if necessary
    pub async fn acquire(path: &Path) -> Self {
        let semaphore = get_file_semaphore(path);
        // The permit borrows from the semaphore, but since semaphore is Arc,
        // the borrow is valid for 'static. We need to extend the lifetime.
        #[allow(clippy::missing_transmute_annotations)]
        let permit = unsafe {
            std::mem::transmute::<_, tokio::sync::SemaphorePermit<'static>>(
                semaphore.acquire().await.unwrap()
            )
        };
        FileLockGuard { _permit: permit }
    }

    /// Try to acquire a lock without waiting
    pub fn try_acquire(path: &Path) -> Option<Self> {
        let semaphore = get_file_semaphore(path);
        // Safety: We're extending the lifetime to 'static, but the permit is actually
        // tied to the Arc<Semaphore> which we keep alive. This is safe as long as
        // the guard doesn't outlive the semaphore.
        semaphore.try_acquire().ok().map(|permit| {
            #[allow(clippy::missing_transmute_annotations)]
            let permit = unsafe {
                std::mem::transmute::<_, tokio::sync::SemaphorePermit<'static>>(permit)
            };
            FileLockGuard { _permit: permit }
        })
    }
}

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        // Permit is automatically released when dropped
    }
}

/// RAII lock guard for file operations
///
/// Holds a file lock for the duration of an operation.
/// The lock is released when this struct is dropped.
pub struct FileLock {
    guard: Option<FileLockGuard>,
}

impl FileLock {
    /// Acquire a file lock, waiting if necessary
    pub async fn lock(path: &Path) -> Self {
        let guard = FileLockGuard::acquire(path).await;
        FileLock { guard: Some(guard) }
    }

    /// Try to acquire a file lock without waiting
    pub fn try_lock(path: &Path) -> Option<Self> {
        let guard = FileLockGuard::try_acquire(path)?;
        Some(FileLock { guard: Some(guard) })
    }

    /// Check if this lock is currently held
    pub fn is_locked(&self) -> bool {
        self.guard.is_some()
    }

    /// Release the lock early (optional - will also release on drop)
    pub fn release(&mut self) {
        self.guard = None;
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // Guard is dropped here, releasing the semaphore permit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_file_lock_basic() {
        let path = PathBuf::from("/tmp/test_file.txt");
        let lock1 = FileLock::lock(&path).await;
        assert!(lock1.is_locked());

        // Try to acquire same file - should succeed after releasing
        drop(lock1);
        let lock2 = FileLock::lock(&path).await;
        assert!(lock2.is_locked());
    }

    #[tokio::test]
    async fn test_file_lock_try_acquire_when_locked() {
        let path = PathBuf::from("/tmp/test_try_acquire.txt");

        let lock1 = FileLock::lock(&path).await;
        assert!(lock1.is_locked());

        // Try to acquire same file - should fail since already locked
        let lock2 = FileLock::try_lock(&path);
        assert!(lock2.is_none());

        drop(lock1);

        // Now should succeed
        let lock3 = FileLock::try_lock(&path);
        assert!(lock3.is_some());
    }

    #[tokio::test]
    async fn test_different_files_can_lock() {
        let path1 = PathBuf::from("/tmp/test_file1.txt");
        let path2 = PathBuf::from("/tmp/test_file2.txt");

        let lock1 = FileLock::lock(&path1).await;
        assert!(lock1.is_locked());

        // Different file should be lockable
        let lock2 = FileLock::lock(&path2).await;
        assert!(lock2.is_locked());
    }

    #[tokio::test]
    async fn test_raii_release_on_drop() {
        let path = PathBuf::from("/tmp/test_raii.txt");
        let path2 = PathBuf::from("/tmp/test_raii2.txt");

        // Lock first file
        {
            let _lock1 = FileLock::lock(&path).await;

            // Now lock second file while first is still locked
            // This should work since they're different files
            let _lock2 = FileLock::lock(&path2).await;
        } // lock1 dropped here

        // Both files should be unlocked now, able to acquire
        let lock1 = FileLock::try_lock(&path);
        let lock2 = FileLock::try_lock(&path2);
        assert!(lock1.is_some());
        assert!(lock2.is_some());
    }
}
