//! Switch-specific extensions to the filesystem ops.

#![stable(feature = "rust1", since = "1.0.0")]

use io;
use fs::File;
use sys_common::{FromInner, AsInner};

/// Switch-specitic extension to [`File`]
///
/// [`File`]: ../../../../std/fs/struct.File.html
#[stable(feature = "rust1", since = "1.0.0")]
pub trait FileExt {
    /// Reopens the underlying file, with its own cursor.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn reopen(&self) -> io::Result<File>;
}

#[stable(feature = "rust1", since = "1.0.0")]
impl FileExt for File {
    fn reopen(&self) -> io::Result<File> {
        Ok(File::from_inner(self.as_inner().reopen()?))
    }
}
