// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use ffi::OsString;
use fmt;
use hash::{Hash, Hasher};
use io::{self, SeekFrom};
use path::{Path, PathBuf, Component};
use sys::time::SystemTime;
use sys::{unsupported, Void};
use sys::os::getcwd;
//use sync::Mutex;
use megaton_hammer::ipcdefs::nn::fssrv::sf::IFileSystemProxy;
use megaton_hammer::kernel::Session;
use megaton_hammer::error::Result as MTHResult;

pub struct File(Box<FileOps>);

#[derive(Debug, Clone)]
pub struct FileAttr {
    size: u64,
    perm: FilePermissions,
    file_type: FileType
}

#[derive(Debug)]
pub struct ReadDir(Box<ReadDirOps<Item = io::Result<DirEntry>>>);

trait ReadDirOps : Iterator + fmt::Debug {}

pub struct DirEntry {
    path: PathBuf,
    file_name: OsString,
    metadata: FileAttr,
    file_type: FileType
}

#[derive(Clone, Debug)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct FilePermissions;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum FileType {
    File,
    Directory
}

#[derive(Debug)]
pub struct DirBuilder { }

impl FileAttr {
    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn perm(&self) -> FilePermissions {
        self.perm
    }

    pub fn file_type(&self) -> FileType {
        self.file_type
    }

    pub fn modified(&self) -> io::Result<SystemTime> {
        unsupported()
    }

    pub fn accessed(&self) -> io::Result<SystemTime> {
        unsupported()
    }

    pub fn created(&self) -> io::Result<SystemTime> {
        unsupported()
    }
}

impl FilePermissions {
    pub fn readonly(&self) -> bool {
        false
    }

    pub fn set_readonly(&mut self, _readonly: bool) {
        // TODO
    }
}

impl FileType {
    pub fn is_dir(&self) -> bool {
        *self == FileType::Directory
    }

    pub fn is_file(&self) -> bool {
        *self == FileType::File
    }

    pub fn is_symlink(&self) -> bool {
        false
    }
}

impl Iterator for ReadDir {
    type Item = io::Result<DirEntry>;

    fn next(&mut self) -> Option<io::Result<DirEntry>> {
        self.0.next()
    }
}

impl DirEntry {
    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub fn file_name(&self) -> OsString {
        self.file_name.clone()
    }

    pub fn metadata(&self) -> io::Result<FileAttr> {
        Ok(self.metadata.clone())
    }

    pub fn file_type(&self) -> io::Result<FileType> {
        Ok(self.file_type.clone())
    }
}

impl OpenOptions {
    pub fn new() -> OpenOptions {
        OpenOptions {
            read: false,
            write: false,
            append: false,
            truncate: false,
            create: false,
            create_new: false
        }
    }

    pub fn read(&mut self, read: bool) {
        self.read = read;
    }
    pub fn write(&mut self, write: bool) {
        self.write = write;
    }
    pub fn append(&mut self, append: bool) {
        self.append = append;
    }
    pub fn truncate(&mut self, truncate: bool) {
        self.truncate = truncate;
    }
    pub fn create(&mut self, create: bool) {
        self.create = create;
    }
    pub fn create_new(&mut self, create_new: bool) {
        self.create_new = create_new;
    }
}

trait FilesystemOps {
    fn open(&self, path: &Path, opts: &OpenOptions) -> io::Result<Box<FileOps>>;
    fn readdir(&self, p: &Path) -> io::Result<ReadDir>;
    fn unlink(&self, p: &Path) -> io::Result<()>;
    fn rename(&self, old: &Path, _new: &Path) -> io::Result<()>;
    fn set_perm(&self, p: &Path, perm: FilePermissions) -> io::Result<()>;
    fn rmdir(&self, p: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
    fn readlink(&self, p: &Path) -> io::Result<PathBuf>;
    fn symlink(&self, src: &Path, dst: &Path) -> io::Result<()>;
    fn link(&self, src: &Path, dst: &Path) -> io::Result<()>;
    fn stat(&self, p: &Path) -> io::Result<FileAttr>;
    fn lstat(&self, p: &Path) -> io::Result<FileAttr>;
    fn canonicalize(&self, p: &Path) -> io::Result<PathBuf>;
}

trait FileOps {
    fn file_attr(&self) -> io::Result<FileAttr>;
    fn fsync(&self) -> io::Result<()>;
    fn datasync(&self) -> io::Result<()>;
    fn truncate(&self, size: u64) -> io::Result<()>;
    fn read(&self, buf: &mut [u8]) -> io::Result<usize>;
    fn write(&self, buf: &[u8]) -> io::Result<usize>;
    fn flush(&self) -> io::Result<()>;
    fn seek(&self, pos: SeekFrom) -> io::Result<u64>;
    // Trait objects: HOW?
    //fn duplicate(&self) -> io::Result<Self>;
    fn set_permissions(&self, perm: FilePermissions) -> io::Result<()>;
}

mod fspsrv {
    use io::{self, ErrorKind};
    use super::{FilesystemOps, FileOps, OpenOptions, FileAttr, SeekFrom, FilePermissions, ReadDir};
    use path::{Path, PathBuf};
    use megaton_hammer::ipcdefs::nn::fssrv::sf::{IFile, IFileSystem};
    use megaton_hammer::kernel::Object;
    use sync::atomic::{AtomicU64, Ordering};
    use sys::switch::unsupported;
    use sys::ext::ffi::OsStrExt;
    use core::slice;

    pub struct FspSrvFs<T>(IFileSystem<T>);

    pub struct FspSrvFile<T> {
        internal: IFile<T>,
        offset: AtomicU64
    }

    impl<T> FspSrvFs<T> {
        pub fn new(fs: IFileSystem<T>) -> FspSrvFs<T> {
            FspSrvFs(fs)
        }
    }

    impl<T: Object + 'static> FilesystemOps for FspSrvFs<T> {
        fn open(&self, path: &Path, opts: &OpenOptions) -> io::Result<Box<FileOps>> {
            let mut arr = [0u8; 0x301];
            let path_as_bytes = path.as_os_str().as_bytes();
            (&mut arr[..path_as_bytes.len()]).copy_from_slice(path_as_bytes);
            let mut mode = 0;
            if opts.read {
                mode |= 1;
            }
            if opts.write {
                mode |= 1 << 1 | 1 << 2;
            }
            if opts.create || opts.create_new {
                let err = self.0.create_file(0, 0, &arr);
                match (opts.create_new, err) {
                    (false, Err(err)) if io::Error::from(err).kind() == ErrorKind::AlreadyExists => (),
                    (_, err) => err?
                }
            }
            let file = self.0.open_file(mode, &arr)?;
            if opts.truncate {
                file.set_size(0)?;
            }
            let offset = if opts.append {
                file.get_size()?
            } else {
                0
            };
            Ok(Box::new(FspSrvFile {
                internal: file,
                offset: AtomicU64::new(offset)
            }))
        }
        fn readdir(&self, p: &Path) -> io::Result<ReadDir> {
            unsupported()
        }
        fn unlink(&self, p: &Path) -> io::Result<()> {
            unsupported()
        }
        fn rename(&self, old: &Path, _new: &Path) -> io::Result<()> {
            unsupported()
        }
        fn set_perm(&self, p: &Path, perm: FilePermissions) -> io::Result<()> {
            unsupported()
        }
        fn rmdir(&self, p: &Path) -> io::Result<()> {
            unsupported()
        }
        fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
            unsupported()
        }
        fn readlink(&self, p: &Path) -> io::Result<PathBuf> {
            unsupported()
        }
        fn symlink(&self, src: &Path, dst: &Path) -> io::Result<()> {
            unsupported()
        }
        fn link(&self, src: &Path, dst: &Path) -> io::Result<()> {
            unsupported()
        }
        fn stat(&self, p: &Path) -> io::Result<FileAttr> {
            unsupported()
        }
        fn lstat(&self, p: &Path) -> io::Result<FileAttr> {
            unsupported()
        }
        fn canonicalize(&self, p: &Path) -> io::Result<PathBuf> {
            unsupported()
        }
    }

    impl<T: Object> FileOps for FspSrvFile<T> {
        fn file_attr(&self) -> io::Result<FileAttr> {
            unsupported()
        }
        fn fsync(&self) -> io::Result<()> {
            unsupported()
        }
        fn datasync(&self) -> io::Result<()> {
            unsupported()
        }
        fn truncate(&self, size: u64) -> io::Result<()> {
            self.internal.set_size(size)?;
            Ok(())
        }
        fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
            // TODO: Maybe I should lock the file on read/write?
            let buf = unsafe {
                // Safety: Do I really need to explain why &[u8] to &[i8] is legal?
                slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut i8, buf.len())
            };
            let read_size = self.internal.read(0, self.offset.load(Ordering::SeqCst), buf.len() as u64, buf)?;
            self.offset.fetch_add(read_size, Ordering::SeqCst);
            Ok(read_size as usize)
        }
        fn write(&self, buf: &[u8]) -> io::Result<usize> {
            // TODO: Maybe I should lock the file on read/write?
            // TODO: In append mode, should I ignore offset and just write from
            // the end?
            let buf = unsafe {
                // Safety: Do I really need to explain why &[u8] to &[i8] is legal?
                slice::from_raw_parts(buf.as_ptr() as *const i8, buf.len())
            };
            self.internal.write(0, self.offset.load(Ordering::SeqCst), buf.len() as u64, buf)?;
            self.offset.fetch_add(buf.len() as u64, Ordering::SeqCst);
            Ok(buf.len())
        }
        fn flush(&self) -> io::Result<()> {
            unsupported()
        }
        fn seek(&self, pos: SeekFrom) -> io::Result<u64> {
            let newpos = match pos {
                SeekFrom::Current(pos) => {
                    self.offset.fetch_update(|v| {
                        let newval = v as i64 + pos;
                        if newval < 0 {
                            None
                        } else {
                            Some(newval as u64)
                        }
                    }, Ordering::SeqCst, Ordering::SeqCst).map_err(|_| io::Error::from(ErrorKind::InvalidInput))?
                }
                SeekFrom::Start(pos) => {
                    self.offset.store(pos, Ordering::SeqCst);
                    pos
                },
                SeekFrom::End(pos) => {
                    let size = self.internal.get_size()?;
                    let newpos = size as i64 + pos;
                    if newpos < 0 {
                        Err(io::Error::from(ErrorKind::InvalidInput))?
                    }
                    self.offset.store(newpos as u64, Ordering::SeqCst);
                    newpos as u64
                }
            };
            Ok(newpos)
        }
        /*fn duplicate(&self) -> io::Result<FspSrvFile<T>> {
            unsupported()
        }*/
        fn set_permissions(&self, perm: FilePermissions) -> io::Result<()> {
            unsupported()
        }
    }
}

use self::fspsrv::FspSrvFs;

lazy_static! {
    static ref SDMC: MTHResult<FspSrvFs<Session>> = {
        let ifs = IFileSystemProxy::new(|init| init(0))?;
        let sdcard = ifs.mount_sd_card()?;
        Ok(FspSrvFs::new(sdcard))
    };
}

fn get_filesystem(path: &Path) -> io::Result<(&'static FilesystemOps, &Path)> {
    assert!(path.is_absolute(), "CWD is not absolute ?");
    let mut iter = path.components();
    let prefix = match iter.next() {
        Some(Component::Prefix(prefix)) => prefix.as_os_str(),
        _ => panic!("If path is absolute, it should start with prefix")
    };
    if prefix == "sdmc:" {
        Ok(((&*SDMC).as_ref().map_err(|v| *v)?, &iter.as_path()))
    } else {
        unsupported()
    }
}

impl File {
    pub fn open(path: &Path, opts: &OpenOptions) -> io::Result<File> {
    	let path = getcwd()?.join(path);
        let (fs, path) = get_filesystem(&path)?;
        Ok(File(fs.open(path, opts)?))
    }

    pub fn file_attr(&self) -> io::Result<FileAttr> {
        self.0.file_attr()
    }

    pub fn fsync(&self) -> io::Result<()> {
        self.0.fsync()
    }

    pub fn datasync(&self) -> io::Result<()> {
        self.0.datasync()
    }

    pub fn truncate(&self, size: u64) -> io::Result<()> {
        self.0.truncate(size)
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    pub fn flush(&self) -> io::Result<()> {
        self.0.flush()
    }

    pub fn seek(&self, pos: SeekFrom) -> io::Result<u64> {
        self.0.seek(pos)
    }

    pub fn duplicate(&self) -> io::Result<File> {
        // Trait objects are not fun
        // Ok(File(self.0.duplicate()?))
        unsupported()
    }

    pub fn set_permissions(&self, perm: FilePermissions) -> io::Result<()> {
        self.0.set_permissions(perm)
    }
}

impl DirBuilder {
    pub fn new() -> DirBuilder {
        DirBuilder { }
    }

    pub fn mkdir(&self, _p: &Path) -> io::Result<()> {
        unsupported()
    }
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "File")
    }
}

pub fn readdir(p: &Path) -> io::Result<ReadDir> {
    let path = getcwd()?.join(p);
    let (fs, path) = get_filesystem(&path)?;
    fs.readdir(path)
}

pub fn unlink(p: &Path) -> io::Result<()> {
    let path = getcwd()?.join(p);
    let (fs, path) = get_filesystem(&path)?;
    fs.unlink(path)
}

pub fn rename(old: &Path, new: &Path) -> io::Result<()> {
    // Guarantee old and new have the same prefix
    let cwd = getcwd()?;
    let old = cwd.join(old);
    let new = cwd.join(new);
    let prefix_old = match old.components().next() {
        Some(Component::Prefix(prefix)) => prefix.as_os_str(),
        _ => panic!("If path is absolute, it should start with prefix")
    };
    let mut newpath_iter = new.components();
    let prefix_new = match newpath_iter.next() {
        Some(Component::Prefix(prefix)) => prefix.as_os_str(),
        _ => panic!("If path is absolute, it should start with prefix")
    };

    if prefix_old != prefix_new {
        // TODO: MTH error
        return Err(io::Error::from(io::ErrorKind::Other));
    }
    let (fs, oldpath) = get_filesystem(&old)?;
    let newpath = newpath_iter.as_path();
    fs.rename(&old, newpath)
}

pub fn set_perm(p: &Path, perm: FilePermissions) -> io::Result<()> {
    let path = getcwd()?.join(p);
    let (fs, path) = get_filesystem(&path)?;
    fs.set_perm(path, perm)
}

pub fn rmdir(p: &Path) -> io::Result<()> {
    let path = getcwd()?.join(p);
    let (fs, path) = get_filesystem(&path)?;
    fs.rmdir(path)
}

pub fn remove_dir_all(p: &Path) -> io::Result<()> {
    let path = getcwd()?.join(p);
    let (fs, path) = get_filesystem(&path)?;
    fs.remove_dir_all(path)
}

pub fn readlink(p: &Path) -> io::Result<PathBuf> {
    let path = getcwd()?.join(p);
    let (fs, path) = get_filesystem(&path)?;
    fs.readlink(path)
}

pub fn symlink(src: &Path, dst: &Path) -> io::Result<()> {
    unsupported()
}

pub fn link(_src: &Path, _dst: &Path) -> io::Result<()> {
    unsupported()
}

pub fn stat(p: &Path) -> io::Result<FileAttr> {
    let path = getcwd()?.join(p);
    let (fs, path) = get_filesystem(&path)?;
    fs.stat(path)
}

pub fn lstat(p: &Path) -> io::Result<FileAttr> {
    let path = getcwd()?.join(p);
    let (fs, path) = get_filesystem(&path)?;
    fs.lstat(path)
}

pub fn canonicalize(p: &Path) -> io::Result<PathBuf> {
    let path = getcwd()?.join(p);
    let (fs, path) = get_filesystem(&path)?;
    fs.canonicalize(path)
}

pub fn copy(from: &Path, to: &Path) -> io::Result<u64> {
    use fs::File;
    if !from.is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
                              "the source path is not an existing regular file"))
    }

    let mut reader = File::open(from)?;
    let mut writer = File::create(to)?;
    let perm = reader.metadata()?.permissions();

    let ret = io::copy(&mut reader, &mut writer)?;
    writer.set_permissions(perm)?;
    Ok(ret)
}
