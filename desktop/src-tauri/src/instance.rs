use std::env;
use std::fs::{self, DirBuilder, File, OpenOptions, TryLockError};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

const LOCK_NAME: &str = "dev.pastop.chatgpt-tray.lock";
// Linux O_NOFOLLOW: never follow a pre-existing lock-file symlink.
const O_NOFOLLOW: i32 = 0x20000;

fn invalid_path() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "Unsafe instance lock location",
    )
}

fn validate_directory(path: &Path, uid: u32, private: bool) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    let forbidden = if private { 0o077 } else { 0o022 };
    if !path.is_absolute()
        || !metadata.is_dir()
        || metadata.uid() != uid
        || metadata.mode() & forbidden != 0
    {
        return Err(invalid_path());
    }
    Ok(())
}

fn create_directory(path: &Path, uid: u32, private: bool) -> io::Result<()> {
    match DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    validate_directory(path, uid, private)
}

fn lock_directory(uid: u32) -> io::Result<PathBuf> {
    if let Some(runtime) = env::var_os("XDG_RUNTIME_DIR") {
        let runtime = PathBuf::from(runtime);
        if validate_directory(&runtime, uid, true).is_ok() {
            return Ok(runtime);
        }
    }

    let home = PathBuf::from(env::var_os("HOME").ok_or_else(invalid_path)?);
    validate_directory(&home, uid, false)?;
    let cache = home.join(".cache");
    create_directory(&cache, uid, false)?;
    let directory = cache.join("chatgpt-tray");
    create_directory(&directory, uid, true)?;
    Ok(directory)
}

fn acquire_in(directory: &Path, uid: u32) -> io::Result<Option<File>> {
    validate_directory(directory, uid, true)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(O_NOFOLLOW)
        .open(directory.join(LOCK_NAME))?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != uid
        || metadata.mode() & 0o077 != 0
        || metadata.nlink() != 1
    {
        return Err(invalid_path());
    }
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

pub fn acquire() -> io::Result<Option<File>> {
    // Linux procfs exposes the effective owner's UID without an extra dependency.
    let uid = fs::metadata("/proc/self")?.uid();
    acquire_in(&lock_directory(uid)?, uid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            loop {
                let path = env::temp_dir().join(format!(
                    "chatgpt-tray-instance-{}-{}",
                    std::process::id(),
                    NEXT_ID.fetch_add(1, Ordering::Relaxed)
                ));
                match DirBuilder::new().mode(0o700).create(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("Cannot create test directory: {error}"),
                }
            }
        }

        fn uid(&self) -> u32 {
            fs::metadata(&self.0).unwrap().uid()
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn separate_handles_contend_and_release_without_removing_file() {
        let directory = TestDirectory::new();
        let first = acquire_in(&directory.0, directory.uid()).unwrap().unwrap();
        assert!(acquire_in(&directory.0, directory.uid()).unwrap().is_none());
        let before = fs::metadata(directory.0.join(LOCK_NAME)).unwrap();
        assert_eq!(before.mode() & 0o777, 0o600);
        drop(first);
        let second = acquire_in(&directory.0, directory.uid()).unwrap().unwrap();
        assert_eq!(before.ino(), second.metadata().unwrap().ino());
    }

    #[test]
    fn rejects_nonprivate_directory() {
        let directory = TestDirectory::new();
        fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(acquire_in(&directory.0, directory.uid()).is_err());
    }

    #[test]
    fn rejects_symlink_and_nonprivate_existing_lock() {
        let directory = TestDirectory::new();
        let lock = directory.0.join(LOCK_NAME);
        symlink(directory.0.join("target"), &lock).unwrap();
        assert!(acquire_in(&directory.0, directory.uid()).is_err());
        fs::remove_file(&lock).unwrap();
        File::create(&lock).unwrap();
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(acquire_in(&directory.0, directory.uid()).is_err());
    }
}
