// SPDX-License-Identifier: GPL-3.0-only
use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, Dirent, DiskInode,
    EasyFileSystem, VfsError, DIRENT_SZ, NAME_LENGTH_LIMIT, S_IFDIR, S_IFMT, S_IFREG,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};

/// Whether `name` fits in one directory entry
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= NAME_LENGTH_LIMIT
}

/// Whether `name` is "." or ".."
fn is_dot(name: &str) -> bool {
    name == "." || name == ".."
}

/// Virtual filesystem layer over easy-fs
pub struct Inode {
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
        }
    }
    /// Call a function over the disk inode to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }
    /// Call a function over the disk inode to modify it
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }
    /// Read the disk inode stored at `pos`, a block id and an offset
    fn read_inode_at<V>(&self, pos: (u32, usize), f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(pos.0 as usize, Arc::clone(&self.block_device))
            .lock()
            .read(pos.1, f)
    }
    /// Modify the disk inode stored at `pos`, a block id and an offset
    fn modify_inode_at<V>(&self, pos: (u32, usize), f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(pos.0 as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(pos.1, f)
    }
    /// Number of entries in a directory inode
    fn entry_count(disk_inode: &DiskInode) -> usize {
        (disk_inode.size as usize) / DIRENT_SZ
    }
    /// Read entry `index` of a directory inode
    fn read_dirent(&self, disk_inode: &DiskInode, index: usize) -> DirEntry {
        let mut dirent = DirEntry::empty();
        assert_eq!(
            disk_inode.read_at(index * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device),
            DIRENT_SZ,
        );
        dirent
    }
    /// Find the entry named `name` in a directory inode
    fn find_entry(&self, name: &str, disk_inode: &DiskInode) -> Option<(usize, u32)> {
        for index in 0..Self::entry_count(disk_inode) {
            let dirent = self.read_dirent(disk_inode, index);
            if dirent.name() == name {
                return Some((index, dirent.inode_number()));
            }
        }
        None
    }
    /// Append an entry to a directory inode, growing it by one entry
    fn append_entry(
        &self,
        disk_inode: &mut DiskInode,
        name: &str,
        inode_id: u32,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        let index = Self::entry_count(disk_inode);
        self.increase_size(((index + 1) * DIRENT_SZ) as u32, disk_inode, fs);
        let dirent = DirEntry::new(name, inode_id);
        disk_inode.write_at(index * DIRENT_SZ, dirent.as_bytes(), &self.block_device);
    }
    /// Remove entry `index`, moving the last entry into its place
    fn remove_entry(&self, disk_inode: &mut DiskInode, index: usize) {
        let last = Self::entry_count(disk_inode) - 1;
        if index != last {
            let dirent = self.read_dirent(disk_inode, last);
            disk_inode.write_at(index * DIRENT_SZ, dirent.as_bytes(), &self.block_device);
        }
        disk_inode.size = (last * DIRENT_SZ) as u32;
    }
    /// Rewrite the ".." entry of the directory inode at `pos` to name `parent_id`
    fn set_parent(&self, pos: (u32, usize), parent_id: u32) {
        self.modify_inode_at(pos, |disk_inode| {
            if let Some((index, _)) = self.find_entry("..", disk_inode) {
                let dirent = DirEntry::new("..", parent_id);
                disk_inode.write_at(index * DIRENT_SZ, dirent.as_bytes(), &self.block_device);
            }
        });
    }
    /// Whether directory `dir_id` is `ancestor_id` or lies below it
    fn is_in_subtree(&self, fs: &EasyFileSystem, dir_id: u32, ancestor_id: u32) -> bool {
        let mut id = dir_id;
        loop {
            if id == ancestor_id {
                return true;
            }
            let parent = self.read_inode_at(fs.get_disk_inode_pos(id), |disk_inode| {
                self.find_entry("..", disk_inode)
                    .map(|(index, _)| self.read_dirent(disk_inode, index).inode_number())
            });
            match parent {
                // the root's ".." names the root itself, which ends the walk
                Some(parent_id) if parent_id != id => id = parent_id,
                _ => return false,
            }
        }
    }
    /// Free the data blocks of the inode `inode_id` and the inode itself
    fn free_inode(&self, fs: &mut MutexGuard<EasyFileSystem>, inode_id: u32) {
        let pos = fs.get_disk_inode_pos(inode_id);
        self.modify_inode_at(pos, |disk_inode| {
            let size = disk_inode.size;
            let blocks = disk_inode.clear_size(&self.block_device);
            assert_eq!(blocks.len(), DiskInode::total_blocks(size) as usize);
            for block in blocks {
                fs.dealloc_data(block);
            }
        });
        fs.dealloc_inode(inode_id);
    }
    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }
    /// Get inode id of current inode
    pub fn inode_id(&self) -> u32 {
        let fs = self.fs.lock();
        fs.get_inode_id(self.block_id as u32, self.block_offset)
    }
    /// Whether current inode is a directory
    pub fn is_dir(&self) -> bool {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.is_dir())
    }
    /// Whether current inode is a regular file
    pub fn is_file(&self) -> bool {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.is_file())
    }
    /// Mode word of the current inode
    pub fn mode(&self) -> u16 {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.mode)
    }
    /// Owner uid of the current inode
    pub fn uid(&self) -> u16 {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.uid)
    }
    /// Owner gid of the current inode
    pub fn gid(&self) -> u16 {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.gid)
    }
    /// Device number of a character or block device inode, zero otherwise
    pub fn dev(&self) -> u32 {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.dev())
    }
    /// Replace the mode word of the current inode
    pub fn set_mode(&self, mode: u16) {
        let _fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| disk_inode.mode = mode);
        block_cache_sync_all();
    }
    /// Replace the owner of the current inode
    pub fn set_owner(&self, uid: u16, gid: u16) {
        let _fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            disk_inode.uid = uid;
            disk_inode.gid = gid;
        });
        block_cache_sync_all();
    }
    /// Size of the current inode in bytes
    pub fn size(&self) -> u32 {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.size)
    }
    /// Hard-link count of the current inode
    pub fn nlink(&self) -> u32 {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.nlink)
    }
    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }
    /// Write data to current inode, growing it eagerly
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }
    /// Free the data blocks of the current inode and set its size to zero
    pub fn truncate(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let blocks = disk_inode.clear_size(&self.block_device);
            assert_eq!(blocks.len(), DiskInode::total_blocks(size) as usize);
            for block in blocks {
                fs.dealloc_data(block);
            }
        });
        block_cache_sync_all();
    }
    /// Find the inode named `name` under the current directory
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            if !disk_inode.is_dir() {
                return None;
            }
            self.find_entry(name, disk_inode).map(|(_, inode_id)| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                ))
            })
        })
    }
    /// Create a regular file named `name` with permission bits `mode`
    pub fn create_file(
        &self,
        name: &str,
        mode: u16,
        uid: u16,
        gid: u16,
    ) -> Result<Arc<Inode>, VfsError> {
        self.create_inode(name, S_IFREG | mode, uid, gid, 0)
    }
    /// Create a directory named `name` with permission bits `mode`
    pub fn create_dir(
        &self,
        name: &str,
        mode: u16,
        uid: u16,
        gid: u16,
    ) -> Result<Arc<Inode>, VfsError> {
        self.create_inode(name, S_IFDIR | mode, uid, gid, 0)
    }
    /// Create a special file named `name`; `mode` carries the full type
    pub fn create_node(
        &self,
        name: &str,
        mode: u16,
        uid: u16,
        gid: u16,
        dev: u32,
    ) -> Result<Arc<Inode>, VfsError> {
        self.create_inode(name, mode, uid, gid, dev)
    }
    /// Create an inode with `mode`, owned by `uid`/`gid`, in the current directory
    fn create_inode(
        &self,
        name: &str,
        mode: u16,
        uid: u16,
        gid: u16,
        dev: u32,
    ) -> Result<Arc<Inode>, VfsError> {
        if !valid_name(name) {
            return Err(VfsError::InvalidName);
        }
        let mut fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            if !disk_inode.is_dir() {
                Err(VfsError::NotDirectory)
            } else if self.find_entry(name, disk_inode).is_some() {
                Err(VfsError::Exists)
            } else {
                Ok(())
            }
        })?;
        let is_dir = mode & S_IFMT == S_IFDIR;
        let parent_id = fs.get_inode_id(self.block_id as u32, self.block_offset);
        let new_inode_id = fs.alloc_inode();
        let new_pos = fs.get_disk_inode_pos(new_inode_id);
        self.modify_inode_at(new_pos, |disk_inode| {
            disk_inode.initialize(mode, uid, gid);
            disk_inode.direct[0] = dev;
        });
        self.modify_disk_inode(|disk_inode| {
            self.append_entry(disk_inode, name, new_inode_id, &mut fs);
            if is_dir {
                disk_inode.nlink += 1;
            }
        });
        if is_dir {
            // a new directory names itself and its parent
            self.modify_inode_at(new_pos, |disk_inode| {
                self.append_entry(disk_inode, ".", new_inode_id, &mut fs);
                self.append_entry(disk_inode, "..", parent_id, &mut fs);
            });
        }
        block_cache_sync_all();
        Ok(Arc::new(Self::new(
            new_pos.0,
            new_pos.1,
            self.fs.clone(),
            self.block_device.clone(),
        )))
    }
    /// Remove the entry `name` from the current directory
    pub fn remove(&self, name: &str) -> Result<(), VfsError> {
        if !valid_name(name) || is_dot(name) {
            return Err(VfsError::InvalidName);
        }
        let mut fs = self.fs.lock();
        let (index, inode_id) = self.read_disk_inode(|disk_inode| {
            if !disk_inode.is_dir() {
                return Err(VfsError::NotDirectory);
            }
            self.find_entry(name, disk_inode).ok_or(VfsError::NotFound)
        })?;
        let pos = fs.get_disk_inode_pos(inode_id);
        let (is_dir, nlink, size) = self.read_inode_at(pos, |disk_inode| {
            (disk_inode.is_dir(), disk_inode.nlink, disk_inode.size)
        });
        if is_dir && size > (2 * DIRENT_SZ) as u32 {
            return Err(VfsError::NotEmpty);
        }
        self.modify_disk_inode(|disk_inode| {
            self.remove_entry(disk_inode, index);
            if is_dir {
                // this entry and the child's ".." were the child's only links
                disk_inode.nlink -= 1;
            }
        });
        if is_dir || nlink == 1 {
            self.free_inode(&mut fs, inode_id);
        } else {
            self.modify_inode_at(pos, |disk_inode| disk_inode.nlink -= 1);
        }
        block_cache_sync_all();
        Ok(())
    }
    /// Link `target` into the current directory as `name`
    pub fn link(&self, name: &str, target: &Arc<Inode>) -> Result<(), VfsError> {
        if !valid_name(name) {
            return Err(VfsError::InvalidName);
        }
        let mut fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            if !disk_inode.is_dir() {
                Err(VfsError::NotDirectory)
            } else if self.find_entry(name, disk_inode).is_some() {
                Err(VfsError::Exists)
            } else {
                Ok(())
            }
        })?;
        let target_id = fs.get_inode_id(target.block_id as u32, target.block_offset);
        let target_pos = fs.get_disk_inode_pos(target_id);
        if self.read_inode_at(target_pos, |disk_inode| disk_inode.is_dir()) {
            return Err(VfsError::IsDirectory);
        }
        self.modify_inode_at(target_pos, |disk_inode| disk_inode.nlink += 1);
        self.modify_disk_inode(|disk_inode| {
            self.append_entry(disk_inode, name, target_id, &mut fs)
        });
        block_cache_sync_all();
        Ok(())
    }
    /// Move `old` from the current directory to `new_dir`/`new`
    pub fn rename(&self, old: &str, new_dir: &Arc<Inode>, new: &str) -> Result<(), VfsError> {
        if !valid_name(old) || !valid_name(new) || is_dot(old) || is_dot(new) {
            return Err(VfsError::InvalidName);
        }
        let mut fs = self.fs.lock();
        let self_id = fs.get_inode_id(self.block_id as u32, self.block_offset);
        let new_dir_id = fs.get_inode_id(new_dir.block_id as u32, new_dir.block_offset);
        let old_id = self.read_disk_inode(|disk_inode| {
            if !disk_inode.is_dir() {
                return Err(VfsError::NotDirectory);
            }
            self.find_entry(old, disk_inode)
                .map(|(_, inode_id)| inode_id)
                .ok_or(VfsError::NotFound)
        })?;
        if !new_dir.read_disk_inode(|disk_inode| disk_inode.is_dir()) {
            return Err(VfsError::NotDirectory);
        }
        let old_pos = fs.get_disk_inode_pos(old_id);
        let old_is_dir = self.read_inode_at(old_pos, |disk_inode| disk_inode.is_dir());
        if old_is_dir && self.is_in_subtree(&fs, new_dir_id, old_id) {
            return Err(VfsError::InvalidName);
        }
        // an existing non-directory target is replaced
        if let Some((target_index, target_id)) =
            new_dir.read_disk_inode(|disk_inode| self.find_entry(new, disk_inode))
        {
            if target_id == old_id {
                return Ok(());
            }
            let target_pos = fs.get_disk_inode_pos(target_id);
            if self.read_inode_at(target_pos, |disk_inode| disk_inode.is_dir()) {
                return Err(VfsError::NotEmpty);
            }
            let target_nlink = self.read_inode_at(target_pos, |disk_inode| disk_inode.nlink);
            new_dir.modify_disk_inode(|disk_inode| self.remove_entry(disk_inode, target_index));
            if target_nlink == 1 {
                self.free_inode(&mut fs, target_id);
            } else {
                self.modify_inode_at(target_pos, |disk_inode| disk_inode.nlink -= 1);
            }
        }
        // removing the target may have moved the old entry, so find it again
        let old_index = self
            .read_disk_inode(|disk_inode| self.find_entry(old, disk_inode).map(|(index, _)| index))
            .ok_or(VfsError::NotFound)?;
        self.modify_disk_inode(|disk_inode| self.remove_entry(disk_inode, old_index));
        if old_is_dir {
            self.set_parent(old_pos, new_dir_id);
            if self_id != new_dir_id {
                // the moved directory is no longer a subdirectory of this one
                self.modify_disk_inode(|disk_inode| disk_inode.nlink -= 1);
                new_dir.modify_disk_inode(|disk_inode| disk_inode.nlink += 1);
            }
        }
        new_dir.modify_disk_inode(|disk_inode| self.append_entry(disk_inode, new, old_id, &mut fs));
        block_cache_sync_all();
        Ok(())
    }
    /// Read the directory entry at byte `offset`
    pub fn dirent_at(&self, offset: usize) -> Option<Dirent> {
        if !offset.is_multiple_of(DIRENT_SZ) {
            return None;
        }
        let index = offset / DIRENT_SZ;
        let (name, inode_id) = self.read_disk_inode(|disk_inode| {
            if !disk_inode.is_dir() || index >= Self::entry_count(disk_inode) {
                return None;
            }
            let dirent = self.read_dirent(disk_inode, index);
            Some((String::from(dirent.name()), dirent.inode_number()))
        })?;
        let pos = self.fs.lock().get_disk_inode_pos(inode_id);
        let is_dir = self.read_inode_at(pos, |disk_inode| disk_inode.is_dir());
        Some(Dirent {
            name,
            inode_id,
            is_dir,
            next_offset: offset + DIRENT_SZ,
        })
    }
}
