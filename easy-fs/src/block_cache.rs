// SPDX-License-Identifier: GPL-3.0-only
use super::{BlockDevice, BLOCK_SZ};
use crate::sync::SpinMutex;
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::alloc::Layout;
use core::mem::ManuallyDrop;
use core::ptr::{addr_of, addr_of_mut};
use core::slice;

/// The number of caches kept in memory, docs/block.md §7
const BLOCK_CACHE_SIZE: usize = 64;
/// The number of hash buckets the caches are queued in
const HASH_BUCKETS: usize = 16;

/// Use `ManuallyDrop` to ensure data is deallocated with an alignment of `BLOCK_SZ`
struct CacheData(ManuallyDrop<Box<[u8; BLOCK_SZ]>>);

impl CacheData {
    pub fn new() -> Self {
        let data = unsafe {
            let raw = alloc::alloc::alloc(Self::layout());
            Box::from_raw(raw as *mut [u8; BLOCK_SZ])
        };
        Self(ManuallyDrop::new(data))
    }

    fn layout() -> Layout {
        Layout::from_size_align(BLOCK_SZ, BLOCK_SZ).unwrap()
    }
}

impl Drop for CacheData {
    fn drop(&mut self) {
        let ptr = self.0.as_mut_ptr();
        unsafe { alloc::alloc::dealloc(ptr, Self::layout()) };
    }
}

impl AsRef<[u8]> for CacheData {
    fn as_ref(&self) -> &[u8] {
        let ptr = self.0.as_ptr();
        unsafe { slice::from_raw_parts(ptr, BLOCK_SZ) }
    }
}

impl AsMut<[u8]> for CacheData {
    fn as_mut(&mut self) -> &mut [u8] {
        let ptr = self.0.as_mut_ptr();
        unsafe { slice::from_raw_parts_mut(ptr, BLOCK_SZ) }
    }
}

/// Cached block inside memory
pub struct BlockCache {
    /// cached block data
    cache: CacheData,
    /// underlying block id
    block_id: usize,
    /// underlying block device
    block_device: Arc<dyn BlockDevice>,
    /// whether the block is dirty
    modified: bool,
}

impl BlockCache {
    /// Load a new BlockCache from disk.
    pub fn new(block_id: usize, block_device: Arc<dyn BlockDevice>) -> Self {
        // for alignment and move effciency
        let mut cache = CacheData::new();
        block_device.read_block(block_id, cache.as_mut());
        Self {
            cache,
            block_id,
            block_device,
            modified: false,
        }
    }
    /// Get the address of an offset inside the cached block data
    fn addr_of_offset(&self, offset: usize) -> *const u8 {
        addr_of!(self.cache.as_ref()[offset])
    }

    fn addr_of_offset_mut(&mut self, offset: usize) -> *mut u8 {
        addr_of_mut!(self.cache.as_mut()[offset])
    }

    pub fn get_ref<T>(&self, offset: usize) -> &T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        let addr = self.addr_of_offset(offset) as *const T;
        unsafe { &*addr }
    }

    pub fn get_mut<T>(&mut self, offset: usize) -> &mut T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        self.modified = true;
        let addr = self.addr_of_offset_mut(offset) as *mut T;
        unsafe { &mut *addr }
    }

    pub fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V {
        f(self.get_ref(offset))
    }

    pub fn modify<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V {
        f(self.get_mut(offset))
    }

    pub fn sync(&mut self) {
        if self.modified {
            self.modified = false;
            self.block_device
                .write_block(self.block_id, self.cache.as_ref());
        }
    }
}

impl Drop for BlockCache {
    fn drop(&mut self) {
        self.sync()
    }
}

/// One cache and the block id it holds
type CacheEntry = (usize, Arc<SpinMutex<BlockCache>>);

/// The caches of a device, in `HASH_BUCKETS` queues of `block_id % HASH_BUCKETS`
struct BlockCacheManager {
    /// The queues; the front of each is the oldest cache
    queues: [VecDeque<CacheEntry>; HASH_BUCKETS],
}

impl BlockCacheManager {
    /// A manager with every bucket empty
    const fn new() -> Self {
        Self {
            queues: [const { VecDeque::new() }; HASH_BUCKETS],
        }
    }

    /// Number of caches held
    fn len(&self) -> usize {
        self.queues.iter().map(VecDeque::len).sum()
    }

    /// Drop the oldest unpinned cache at or after bucket `start`, wrapping around
    fn evict(&mut self, start: usize) {
        for offset in 0..HASH_BUCKETS {
            let bucket = (start + offset) % HASH_BUCKETS;
            let oldest = self.queues[bucket]
                .iter()
                .position(|(_, cache)| Arc::strong_count(cache) == 1);
            if let Some(index) = oldest {
                // the last reference: dropping it writes a dirty cache back
                self.queues[bucket].remove(index);
                return;
            }
        }
        panic!("all {} block caches are pinned", BLOCK_CACHE_SIZE);
    }

    /// Return the cache of `block_id`, loading it on a miss
    fn get_block_cache(
        &mut self,
        block_id: usize,
        block_device: Arc<dyn BlockDevice>,
    ) -> Arc<SpinMutex<BlockCache>> {
        let bucket = block_id % HASH_BUCKETS;
        if let Some((_, cache)) = self.queues[bucket].iter().find(|(id, _)| *id == block_id) {
            return Arc::clone(cache);
        }
        if self.len() == BLOCK_CACHE_SIZE {
            self.evict(bucket);
        }
        let cache = Arc::new(SpinMutex::new(BlockCache::new(block_id, block_device)));
        self.queues[bucket].push_back((block_id, Arc::clone(&cache)));
        cache
    }
}

/// The global block cache manager
static BLOCK_CACHE_MANAGER: SpinMutex<BlockCacheManager> = SpinMutex::new(BlockCacheManager::new());

/// Get the block cache corresponding to the given block id and block device
pub fn get_block_cache(
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
) -> Arc<SpinMutex<BlockCache>> {
    BLOCK_CACHE_MANAGER
        .lock()
        .get_block_cache(block_id, block_device)
}

/// Write every dirty cache back to the block device, docs/block.md §7
pub fn block_cache_sync_all() {
    let manager = BLOCK_CACHE_MANAGER.lock();
    for (_, cache) in manager.queues.iter().flatten() {
        cache.lock().sync();
    }
}
