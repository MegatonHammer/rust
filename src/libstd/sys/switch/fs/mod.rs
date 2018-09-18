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
use fmt::{self, Debug};
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
        Ok(self.metadata.file_type)
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
    fn stat(&self, p: &Path) -> io::Result<FileAttr>;
    fn lstat(&self, p: &Path) -> io::Result<FileAttr>;
    fn canonicalize(&self, p: &Path) -> io::Result<PathBuf>;
}

trait FileOps: Debug {
    fn file_attr(&self) -> io::Result<FileAttr>;
    fn fsync(&self) -> io::Result<()>;
    fn datasync(&self) -> io::Result<()>;
    fn truncate(&self, size: u64) -> io::Result<()>;
    fn read(&self, buf: &mut [u8]) -> io::Result<usize>;
    fn write(&self, buf: &[u8]) -> io::Result<usize>;
    fn flush(&self) -> io::Result<()>;
    fn seek(&self, pos: SeekFrom) -> io::Result<u64>;
    fn duplicate(&self) -> io::Result<Box<FileOps>>;
    // Switch-specific extension. Reopens the file. The file cursor will be
    // different.
    fn reopen(&self) -> io::Result<Box<FileOps>>;
    fn set_permissions(&self, perm: FilePermissions) -> io::Result<()>;
}

mod fspsrv;
mod romfs;

use self::fspsrv::FspSrvFs;

lazy_static! {
    static ref SDMC: MTHResult<FspSrvFs<Session>> = {
        let ifs = IFileSystemProxy::new(|init| init(0))?;
        let sdcard = ifs.open_sd_card_file_system()?;
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

    pub fn reopen(&self) -> io::Result<File> {
        Ok(File(self.0.reopen()?))
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
        Ok(File(self.0.duplicate()?))
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
    let (fs, fs_path) = get_filesystem(&path)?;
    let canonicalized = fs.canonicalize(fs_path)?;

    let mut iter = path.components();
    let prefix = match iter.next() {
        Some(Component::Prefix(prefix)) => prefix.as_os_str(),
        _ => panic!("If path is absolute, it should start with prefix")
    };
    let mut ret = PathBuf::from(prefix);
    ret.push(canonicalized);
    Ok(ret)
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
