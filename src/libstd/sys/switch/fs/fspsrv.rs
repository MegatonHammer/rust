use io::{self, ErrorKind};
use super::{FilesystemOps, FileOps, OpenOptions, FileAttr, SeekFrom, FilePermissions, ReadDir, FileType,DirEntry, ReadDirOps};
use path::{Path, PathBuf, Component};
use ffi::OsStr;
use megaton_hammer::ipcdefs::nn::fssrv::sf::{IFile, IDirectory, IFileSystem, IDirectoryEntry, DirectoryEntryType};
use megaton_hammer::kernel::Object;
use sync::atomic::{AtomicU64, Ordering};
use sync::Arc;
use sys::switch::unsupported;
use sys::ext::ffi::OsStrExt;
use sys_common::AsInner;
use core::slice;
use core::fmt::Debug;

pub struct FspSrvFs<T>(IFileSystem<T>);

#[derive(Debug)]
pub struct FspSrvFile<T: Debug> {
    internal: Arc<IFile<T>>,
    offset: AtomicU64
}

impl<T> FspSrvFs<T> {
    pub fn new(fs: IFileSystem<T>) -> FspSrvFs<T> {
        FspSrvFs(fs)
    }
}

#[derive(Debug)]
pub struct FspReadDir<T> {
    internal: IDirectory<T>,
    parent: PathBuf
}

impl<T: Object + Debug> ReadDirOps for FspReadDir<T> {}
impl<T: Object> Iterator for FspReadDir<T> {
    type Item = io::Result<DirEntry>;

    fn next(&mut self) -> Option<io::Result<DirEntry>> {
        let mut entry: [IDirectoryEntry; 1] = [IDirectoryEntry {
            path: [0; 0x300],
            unk1: 0,
            directory_entry_type: DirectoryEntryType::File,
            filesize: 0,
        }];
        match self.internal.read(&mut entry) {
            Ok(0) => return None,
            Err(err) => return Some(Err(err.into())),
            Ok(n) => ()
        }
        let size = entry[0].path.iter().position(|c| *c == b'\0').unwrap_or(0x300);
        let file_name = OsStr::from_bytes(&entry[0].path[..size]);
        Some(Ok(DirEntry {
            path: self.parent.join(file_name),
            file_name: file_name.into(),
            metadata: FileAttr {
                size: entry[0].filesize,
                perm: FilePermissions,
                file_type: entry[0].directory_entry_type.into()
            },
        }))
    }
}

impl From<DirectoryEntryType> for FileType {
    fn from(ty: DirectoryEntryType) -> FileType {
        match ty {
            DirectoryEntryType::File => FileType::File,
            DirectoryEntryType::Directory => FileType::Directory
        }
    }
}

impl<T: Object + Debug + 'static> FilesystemOps for FspSrvFs<T> {
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
            internal: Arc::new(file),
            offset: AtomicU64::new(offset)
        }))
    }
    fn readdir(&self, p: &Path) -> io::Result<ReadDir> {
        let mut arr = [0u8; 0x301];
        let path_as_bytes = p.as_os_str().as_bytes();
        (&mut arr[..path_as_bytes.len()]).copy_from_slice(path_as_bytes);
        Ok(ReadDir(Box::new(FspReadDir {
            internal: self.0.open_directory(3, &arr)?,
            parent: p.into()
        })))
    }
    fn unlink(&self, p: &Path) -> io::Result<()> {
        let mut arr = [0u8; 0x301];
        let path_as_bytes = p.as_os_str().as_bytes();
        (&mut arr[..path_as_bytes.len()]).copy_from_slice(path_as_bytes);
        self.0.delete_file(&arr)?;
        Ok(())
    }
    fn rename(&self, old: &Path, new: &Path) -> io::Result<()> {
        let mut oldarr = [0u8; 0x301];
        let path_as_bytes = old.as_os_str().as_bytes();
        (&mut oldarr[..path_as_bytes.len()]).copy_from_slice(path_as_bytes);

        let mut newarr = [0u8; 0x301];
        let path_as_bytes = new.as_os_str().as_bytes();
        (&mut newarr[..path_as_bytes.len()]).copy_from_slice(path_as_bytes);
        self.0.rename_file(&oldarr, &newarr)?;
        Ok(())
    }
    fn set_perm(&self, p: &Path, perm: FilePermissions) -> io::Result<()> {
        unsupported()
    }
    fn rmdir(&self, p: &Path) -> io::Result<()> {
        let mut arr = [0u8; 0x301];
        let path_as_bytes = p.as_os_str().as_bytes();
        (&mut arr[..path_as_bytes.len()]).copy_from_slice(path_as_bytes);
        self.0.delete_directory(&arr)?;
        Ok(())
    }
    fn remove_dir_all(&self, p: &Path) -> io::Result<()> {
        let mut arr = [0u8; 0x301];
        let path_as_bytes = p.as_os_str().as_bytes();
        (&mut arr[..path_as_bytes.len()]).copy_from_slice(path_as_bytes);
        self.0.delete_directory_recursively(&arr)?;
        Ok(())
    }
    fn readlink(&self, p: &Path) -> io::Result<PathBuf> {
        unsupported()
    }
    fn stat(&self, p: &Path) -> io::Result<FileAttr> {
        let mut arr = [0u8; 0x301];
        let path_as_bytes = p.as_os_str().as_bytes();
        (&mut arr[..path_as_bytes.len()]).copy_from_slice(path_as_bytes);

        let entry_type = self.0.get_entry_type(&arr)?;

        let size = match entry_type {
            DirectoryEntryType::File => {
                self.0.open_file(0, &arr)?.get_size()?
            },
            DirectoryEntryType::Directory => 0
        };
        Ok(FileAttr {
            size,
            perm: FilePermissions,
            file_type: entry_type.into()
        })
    }
    fn lstat(&self, p: &Path) -> io::Result<FileAttr> {
        unsupported()
    }
    fn canonicalize(&self, p: &Path) -> io::Result<PathBuf> {
        let mut fullpath = PathBuf::from("/");
        for component in p.components() {
            match component {
                Component::Prefix(p) => panic!("Shouldn't obtain a prefix in inner fs impl"),
                Component::RootDir => fullpath.push("/"),
                Component::CurDir => (),
                Component::ParentDir => { fullpath.pop(); },
                Component::Normal(p) => fullpath.push(p)
            }
        }
        Ok(p.into())
    }
}

impl<T: 'static + Object + Debug> FileOps for FspSrvFile<T> {
    fn file_attr(&self) -> io::Result<FileAttr> {
        Ok(FileAttr {
            size: self.internal.get_size()?,
            perm: FilePermissions,
            file_type: FileType::File
        })
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
        let read_size = self.internal.read(0, self.offset.load(Ordering::SeqCst), buf.len() as u64, buf)?;
        self.offset.fetch_add(read_size, Ordering::SeqCst);
        Ok(read_size as usize)
    }
    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        // TODO: Maybe I should lock the file on read/write?
        // TODO: In append mode, should I ignore offset and just write from
        // the end?
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
    fn duplicate(&self) -> io::Result<Box<FileOps>> {
        // TODO: This requires sharing the cursor between both FDs. That's a huge pain,
        // so let's just not support it for now. We have a custom function called reopen
        // that is basically equivalent to duplicate, but doesn't share the cursor.
        unsupported()
    }
    // Custom extension
    fn reopen(&self) -> io::Result<Box<FileOps>> {
        Ok(Box::new(FspSrvFile {
            internal: self.internal.clone(),
            offset: AtomicU64::new(0)
        }))
    }
    fn set_permissions(&self, perm: FilePermissions) -> io::Result<()> {
        unsupported()
    }
}
