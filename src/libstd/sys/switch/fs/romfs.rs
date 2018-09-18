use super::fspsrv::FspSrvFile;
use super::{FilePermissions, FileAttr, DirEntry, ReadDirOps, ReadDir, FileType, FilesystemOps, FileOps, OpenOptions, unsupported};
use io::{self, SeekFrom};
use path::{Path, PathBuf, Component};
use ffi::OsStr;
use sys::ext::ffi::OsStrExt;
use self::util::*;
use sync::Arc;
use sync::atomic::{AtomicU64, Ordering};

use megaton_hammer::error::MegatonHammerDescription;

extern crate plain;
use self::plain::Plain;

const ROMFS_NONE: u32 = <u32>::max_value();

#[derive(Debug)]
enum RomFsType {
    File {
        f: Box<FileOps>,
        offset: u64
    },
    // TODO: Write a storage backend for romfs.
    Storage(!)
}

impl RomFsType {
    fn read(&self, at: u64, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            &RomFsType::File { ref f, offset } => {
                f.seek(SeekFrom::Start(offset + at))?;
                f.read(buf)
            }
        }
    }

    fn read_exact(&self, at: u64, mut buf: &mut [u8]) -> io::Result<()> {
        while !buf.is_empty() {
            match self.read(at, buf) {
                Ok(0) => break,
                Ok(n) => { let tmp = buf; buf = &mut tmp[n..]; }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        if !buf.is_empty() {
            Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                           "failed to fill whole buffer"))
        } else {
            Ok(())
        }
    }

    fn read_plain<T: Plain>(&self, at: u64, t: &mut T) -> io::Result<()> {
        let data = unsafe {
            // Safety: According to plain, writing to this reference is safe, reading is not.
            // We are only going to write to it, so it should be fine.
            plain::as_mut_bytes(t)
        };
        self.read_exact(at, data)
    }

    fn read_plain_slice<T: Plain>(&self, at: u64, t: &mut [T]) -> io::Result<()> {
        let data = unsafe {
            // Safety: According to plain, writing to this reference is safe, reading is not.
            // We are only going to write to it, so it should be fine.
            plain::as_mut_bytes(t)
        };
        self.read_exact(at, data)
    }

    fn reopen(&self) -> io::Result<RomFsType> {
        match self {
            RomFsType::File { f, offset } => Ok(RomFsType::File {
                f: f.reopen()?, offset: *offset
            })
        }
    }
}


#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct RomFsHeader {
    header_size: u64,
    dir_hash_table_off: u64,
    dir_hash_table_size: u64,
    dir_table_off: u64,
    dir_table_size: u64,
    file_hash_table_off: u64,
    file_hash_table_size: u64,
    file_table_off: u64,
    file_table_size: u64,
    file_data_off: u64,
}
unsafe impl Plain for RomFsHeader {}

trait NextHash {
    fn get_next_hash(&self) -> u32;
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct RomFsDirTableEntryHeader {
    parent: u32,
    sibling: u32,
    child_dir: u32,
    child_file: u32,
    next_hash: u32,
    name_len: u32,
}

impl UnsizedHeader for RomFsDirTableEntryHeader {
    fn get_len(&self) -> usize {
        self.name_len as usize
    }
}

#[repr(C)]
#[derive(Debug)]
struct RomFsDirTableEntry {
    parent: u32,
    sibling: u32,
    child_dir: u32,
    child_file: u32,
    next_hash: u32,
    name_len: u32,
    name: [u8]
}

impl NextHash for RomFsDirTableEntry {
    fn get_next_hash(&self) -> u32 {
        self.next_hash
    }
}

impl Unsized for RomFsDirTableEntry {
    type Header = RomFsDirTableEntryHeader;
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct RomFsFileTableEntryHeader {
    parent: u32,
    sibling: u32,
    data_off: u64,
    data_size: u64,
    next_hash: u32,
    name_len: u32
}

impl UnsizedHeader for RomFsFileTableEntryHeader {
    fn get_len(&self) -> usize {
        self.name_len as usize
    }
}


#[repr(C)]
#[derive(Debug)]
struct RomFsFileTableEntry {
    parent: u32,
    sibling: u32,
    data_off: u64,
    data_size: u64,
    next_hash: u32,
    name_len: u32,
    name: [u8]
}

impl NextHash for RomFsFileTableEntry {
    fn get_next_hash(&self) -> u32 {
        self.next_hash
    }
}

impl Unsized for RomFsFileTableEntry {
    type Header = RomFsFileTableEntryHeader;
}

struct IterateHash<'a, T: 'a + Unsized + ?Sized> {
    ctx: &'a EntityTable<T>, cur_off: u32
}

impl<'a, T: 'a + Unsized + ?Sized> IterateHash<'a, T> {
    fn new(ctx: &'a EntityTable<T>, cur_off: u32) -> IterateHash<'a, T> {
        IterateHash { ctx, cur_off }
    }
}

impl<'a, T: 'a + NextHash + Unsized + ?Sized> Iterator for IterateHash<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        if self.cur_off == ROMFS_NONE {
            None
        } else {
            let cur_ent = self.ctx.get(self.cur_off);
            self.cur_off = cur_ent.get_next_hash();
            Some(cur_ent)
        }
    }
}



#[derive(Debug)]
pub struct RomFsFile {
    internal: RomFsType,
    start: u64,
    size: u64,
    offset: AtomicU64
}

fn calc_hash(parent: u32, name: &OsStr, hash_table_size: u32) -> u32 {
    let mut hash = parent ^ 123456789;
    for c in name.as_bytes() {
        hash = (hash >> 5) | (hash << 27);
        hash ^= *c as u32;
    }
    return hash % hash_table_size;
}

struct RomFs {
    ty: RomFsType,
    header: RomFsHeader,
    dir_hash_table: Vec<u32>,
    dir_table: Arc<EntityTable<RomFsDirTableEntry>>,
    file_hash_table: Vec<u32>,
    file_table: Arc<EntityTable<RomFsFileTableEntry>>
}

impl RomFs {
    pub fn from_file(file: Box<FileOps>, romfs_start_offset: u64) -> io::Result<RomFs> {
        let romfs_type = RomFsType::File {
            f: file,
            offset: romfs_start_offset
        };

        let mut header: RomFsHeader = RomFsHeader::default();
        romfs_type.read_plain(0, &mut header)?;

        let mut dir_hash_table = vec![0u32; (header.dir_hash_table_size / 4) as usize];
        romfs_type.read_plain_slice(header.dir_hash_table_off, &mut dir_hash_table)?;

        let mut dir_table = vec![0; header.dir_table_size as usize];
        romfs_type.read_exact(header.dir_table_off, &mut dir_table)?;

        let mut file_hash_table = vec![0u32; (header.file_hash_table_size / 4) as usize];
        romfs_type.read_plain_slice(header.file_hash_table_off, &mut file_hash_table)?;

        let mut file_table = vec![0; header.file_table_size as usize];
        romfs_type.read_exact(header.file_table_off, &mut file_table)?;

        Ok(RomFs {
            ty: romfs_type,
            header,
            dir_hash_table,
            dir_table: Arc::new(EntityTable::new(dir_table)),
            file_hash_table,
            file_table: Arc::new(EntityTable::new(file_table))
        })
    }

    pub fn get_dir(&self, idx: u32) -> &RomFsDirTableEntry {
        self.dir_table.get(idx)
    }

    pub fn get_file(&self, idx: u32) -> &RomFsFileTableEntry {
        self.file_table.get(idx)
    }

    fn root_dir(&self) -> &RomFsDirTableEntry {
        self.get_dir(0)
    }

    fn search_for_dir(&self, parent: &RomFsDirTableEntry, path_component: &OsStr) -> Option<&RomFsDirTableEntry> {
        let parent_off = parent as *const RomFsDirTableEntry as *const u8 as u32 - self.dir_table.as_ptr() as u32;
        let hash = calc_hash(parent_off, path_component, self.dir_hash_table.len() as u32);

        for cur_dir in IterateHash::new(&*self.dir_table, self.dir_hash_table[hash as usize]) {
            if cur_dir.parent != parent_off { continue }
            if &cur_dir.name != path_component.as_bytes() { continue }
            return Some(cur_dir)
        }

        None
    }

    fn search_for_file(&self, parent: &RomFsDirTableEntry, path_component: &OsStr) -> Option<&RomFsFileTableEntry> {
        let parent_off = parent as *const RomFsDirTableEntry as *const u8 as u32 - self.dir_table.as_ptr() as u32;
        let hash = calc_hash(parent_off, path_component, self.dir_hash_table.len() as u32);

        for cur_file in IterateHash::new(&*self.file_table, self.file_hash_table[hash as usize]) {
            if cur_file.parent != parent_off { continue }
            if &cur_file.name != path_component.as_bytes() { continue }
            return Some(cur_file)
        }

        None
    }

    fn navigate_to_dir<'a>(&'a self, mut parent: &'a RomFsDirTableEntry, path: &Path) -> io::Result<&'a RomFsDirTableEntry> {
        let mut components = path.components();
        assert_eq!(Some(Component::RootDir), components.next(), "Expected first component of path to be root dir");
        for component in components {
            match component {
                Component::Prefix(_) | Component::RootDir => panic!("Didn't expect component here"),
                Component::CurDir => continue,
                Component::ParentDir => parent = self.get_dir(parent.parent),
                Component::Normal(p) => {
                    if let Some(new_parent) = self.search_for_dir(parent, p) {
                        parent = new_parent
                    } else {
                        Err(MegatonHammerDescription::RomFsEntityDoesNotExist)?
                    }
                }
            }
        }
        Ok(parent)
    }
}

impl FilesystemOps for RomFs {
    fn open(&self, path: &Path, opts: &OpenOptions) -> io::Result<Box<FileOps>> {
        if opts.write || opts.append || opts.truncate {
            Err(MegatonHammerDescription::RomFsReadOnly)?
        }
        let path_parent = path.parent().ok_or(MegatonHammerDescription::RomFsEntityDoesNotExist)?;
        let parent = self.navigate_to_dir(self.root_dir(), path_parent)?;

        let path_filename = path.file_name().ok_or(MegatonHammerDescription::RomFsEntityDoesNotExist)?;
        if let Some(file) = self.search_for_file(parent, path_filename) {
            if opts.create && opts.create_new {
                Err(MegatonHammerDescription::RomFsEntityExists)?
            } else {
                Ok(Box::new(RomFsFile {
                    internal: self.ty.reopen()?,
                    start: self.header.file_data_off + file.data_off,
                    size: file.data_size,
                    offset: AtomicU64::new(0)
                }))
            }
        } else if opts.create {
            Err(MegatonHammerDescription::RomFsReadOnly)?
        } else {
            Err(MegatonHammerDescription::RomFsEntityDoesNotExist)?
        }
    }

    fn readdir(&self, p: &Path) -> io::Result<ReadDir> {
        let dir = self.navigate_to_dir(self.root_dir(), p)?;

        Ok(ReadDir(Box::new(RomFsReadDir {
            parent_path: PathBuf::from(p),
            dir_table: self.dir_table.clone(),
            file_table: self.file_table.clone(),
            cur_dir: dir.child_dir,
            cur_file: dir.child_file
        })))
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

impl FileOps for RomFsFile {
    fn file_attr(&self) -> io::Result<FileAttr> {
        Ok(FileAttr {
            size: self.size,
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
        unsupported()
    }
    fn read(&self, mut buf: &mut [u8]) -> io::Result<usize> {
        let pos = self.offset.load(Ordering::SeqCst);
        if self.size < pos + buf.len() as u64 {
            let tmp = buf;
            buf = &mut tmp[..(self.size - pos) as usize];
        }
        let out = self.internal.read(self.start + pos, buf)?;
        self.offset.store(pos + out as u64, Ordering::SeqCst);
        Ok(out)
    }
    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        unsupported()
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
                }, Ordering::SeqCst, Ordering::SeqCst).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?
            }
            SeekFrom::Start(pos) => {
                self.offset.store(pos, Ordering::SeqCst);
                pos
            },
            SeekFrom::End(pos) => {
                let size = self.size;
                let newpos = size as i64 + pos;
                if newpos < 0 {
                    Err(io::Error::from(io::ErrorKind::InvalidInput))?
                }
                self.offset.store(newpos as u64, Ordering::SeqCst);
                newpos as u64
            }
        };
        Ok(newpos)
    }
    fn duplicate(&self) -> io::Result<Box<FileOps>> {
        unsupported()
    }
    fn reopen(&self) -> io::Result<Box<FileOps>> {
        unsupported()
    }
    fn set_permissions(&self, perm: FilePermissions) -> io::Result<()> {
        unsupported()
    }
}

#[derive(Debug)]
struct RomFsReadDir {
    file_table: Arc<EntityTable<RomFsFileTableEntry>>,
    dir_table: Arc<EntityTable<RomFsDirTableEntry>>,
    parent_path: PathBuf,
    cur_dir: u32,
    cur_file: u32
}

impl ReadDirOps for RomFsReadDir {}
impl Iterator for RomFsReadDir {
    type Item = io::Result<DirEntry>;

    fn next(&mut self) -> Option<io::Result<DirEntry>> {
        match (self.cur_dir, self.cur_file) {
            (ROMFS_NONE, ROMFS_NONE) => None,
            (ROMFS_NONE, cur_file) => {
                let cur_file = self.file_table.get(cur_file);
                self.cur_file = cur_file.sibling;
                Some(Ok(DirEntry {
                    path: self.parent_path.join(OsStr::from_bytes(&cur_file.name)),
                    file_name: OsStr::from_bytes(&cur_file.name).into(),
                    metadata: FileAttr {
                        size: cur_file.data_size,
                        perm: FilePermissions,
                        file_type: FileType::File
                    }
                }))
            },
            (cur_dir, _) => {
                let cur_dir = self.dir_table.get(cur_dir);
                self.cur_dir = cur_dir.sibling;
                Some(Ok(DirEntry {
                    path: self.parent_path.join(OsStr::from_bytes(&cur_dir.name)),
                    file_name: OsStr::from_bytes(&cur_dir.name).into(),
                    metadata: FileAttr {
                        size: 0,
                        perm: FilePermissions,
                        file_type: FileType::Directory
                    },
                }))
            },
        }
    }
}

mod util {
    //! Stop here you fool! This is a place of madness...
    //!
    //! Look at https://play.rust-lang.org/?gist=e3cc82ffe60c1eef1401e6e15c8875e8&version=stable&mode=debug&edition=2015
    //! if you want a chance at understanding the madness going on here.
    #![allow(dead_code)]

    use core::marker::PhantomData;

    struct FatPtr {
        data: *const u8,
        len: usize
    }

    pub trait Unsized {
        type Header: UnsizedHeader;
    }

    pub trait UnsizedHeader {
        fn get_len(&self) -> usize;
    }

    #[derive(Debug)]
    pub struct EntityTable<T: Unsized + ?Sized> {
        data: Vec<u8>,
        phantom: PhantomData<T>
    }

    impl<T: Unsized + ?Sized> EntityTable<T> {
        pub fn new(data: Vec<u8>) -> EntityTable<T> {
            EntityTable {
                data: data,
                phantom: PhantomData
            }
        }

        pub fn get(&self, idx: u32) -> &T {
            unsafe {
                // Safety:
                // This method is unsafe. It creates a reference with utter
                // disregard for alignment constraints. This function will only be
                // used on AArch64, where unaligned accesses don't cause too much
                // trouble. Still, this is technically UB...

                // Alternative: I could use read_unaligned() and return an owned
                // struct.
                let dir_ptr = (&self.data[idx as usize..]).as_ptr() as *const u8 as *const T::Header;
                let len = (*dir_ptr).get_len();
                let ptr = FatPtr {
                    data: dir_ptr as *const u8,
                    len
                };
                use mem::{self, size_of};
                assert_eq!(size_of::<&T>(), size_of::<FatPtr>(), "&T is not a fat pointer");
                mem::transmute_copy::<FatPtr, &T>(&ptr)
            }
        }

        pub fn as_ptr(&self) -> *const u8 {
            self.data.as_ptr()
        }
    }
}
