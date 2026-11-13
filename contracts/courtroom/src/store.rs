//! Storage maps with exactly the on-chain layout of near_sdk::store::LookupMap and IterableMap
//! (same state bytes, same storage keys, same stored values), without their in-memory caches.
//! Every insert is written to storage straight away instead of when the call ends.
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::env;
use std::io;
use std::marker::PhantomData;

const ERR_DE: &str = "Cannot deserialize element";

fn read<V: BorshDeserialize>(key: &[u8]) -> Option<V> {
    env::storage_read(key).map(|b| V::try_from_slice(&b).unwrap_or_else(|_| env::panic_str(ERR_DE)))
}

fn write<V: BorshSerialize>(key: &[u8], v: &V) {
    env::storage_write(key, &borsh::to_vec(v).unwrap_or_else(|_| env::abort()));
}

fn read_prefix<R: io::Read>(r: &mut R) -> io::Result<Box<[u8]>> {
    Box::<[u8]>::deserialize_reader(r)
}

/// Same layout as `near_sdk::store::LookupMap<K, V>`: the key is the prefix followed by the borsh bytes of K,
/// and the contract state holds only the prefix.
pub struct LookupMap<K, V> {
    prefix: Box<[u8]>,
    kv: PhantomData<(K, V)>,
}

impl<K: BorshSerialize, V: BorshSerialize + BorshDeserialize> LookupMap<K, V> {
    pub fn new(prefix: &[u8]) -> Self {
        Self { prefix: prefix.into(), kv: PhantomData }
    }

    fn key(&self, k: &K) -> Vec<u8> {
        let mut b = self.prefix.to_vec();
        k.serialize(&mut b).unwrap_or_else(|_| env::abort());
        b
    }

    pub fn get(&self, k: &K) -> Option<V> {
        read(&self.key(k))
    }

    pub fn insert(&mut self, k: K, v: V) {
        write(&self.key(&k), &v);
    }
}

impl<K, V> BorshSerialize for LookupMap<K, V> {
    fn serialize<W: io::Write>(&self, w: &mut W) -> io::Result<()> {
        self.prefix.serialize(w)
    }
}

impl<K, V> BorshDeserialize for LookupMap<K, V> {
    fn deserialize_reader<R: io::Read>(r: &mut R) -> io::Result<Self> {
        Ok(Self { prefix: read_prefix(r)?, kv: PhantomData })
    }
}

/// What IterableMap stores for each value: the value, then its place in the key list.
#[derive(BorshSerialize, BorshDeserialize)]
#[borsh(crate = "near_sdk::borsh")]
struct ValueAndIndex<V> {
    value: V,
    key_index: u32,
}

/// Same layout as `near_sdk::store::IterableMap<u32, V>` (sha256 keys): a key list under prefix+"v"
/// (prefix, "v", index as 4 little-endian bytes) and the values under sha256(prefix, "m", key).
/// The contract state holds the key count and both prefixes. Entries are never removed.
pub struct IterableMap<V> {
    len: u32,
    keys_prefix: Box<[u8]>,
    values_prefix: Box<[u8]>,
    v: PhantomData<V>,
}

impl<V: BorshSerialize + BorshDeserialize> IterableMap<V> {
    pub fn new(prefix: &[u8]) -> Self {
        Self {
            len: 0,
            keys_prefix: [prefix, b"v"].concat().into(),
            values_prefix: [prefix, b"m"].concat().into(),
            v: PhantomData,
        }
    }

    fn value_key(&self, k: u32) -> [u8; 32] {
        env::sha256_array([&self.values_prefix[..], &k.to_le_bytes()].concat())
    }

    fn entry(&self, k: u32) -> Option<ValueAndIndex<V>> {
        read(&self.value_key(k))
    }

    pub fn get(&self, k: u32) -> Option<V> {
        self.entry(k).map(|e| e.value)
    }

    /// Replace the value of an existing key, or add the key at the end of the key list.
    pub fn insert(&mut self, k: u32, value: V) {
        let key_index = match self.entry(k) {
            Some(e) => e.key_index,
            None => {
                let i = self.len;
                write(&[&self.keys_prefix[..], &i.to_le_bytes()].concat(), &k);
                self.len += 1;
                i
            }
        };
        write(&self.value_key(k), &ValueAndIndex { value, key_index });
    }

    /// Every value in the order the keys were first added.
    pub fn values(&self) -> Vec<V> {
        (0..self.len)
            .map(|i| {
                let k: u32 = read(&[&self.keys_prefix[..], &i.to_le_bytes()].concat())
                    .unwrap_or_else(|| env::panic_str("The collection is in an inconsistent state. Did previous smart contract execution terminate unexpectedly?"));
                self.get(k).unwrap_or_else(|| env::panic_str("The collection is in an inconsistent state. Did previous smart contract execution terminate unexpectedly?"))
            })
            .collect()
    }
}

impl<V> BorshSerialize for IterableMap<V> {
    fn serialize<W: io::Write>(&self, w: &mut W) -> io::Result<()> {
        self.len.serialize(w)?;
        self.keys_prefix.serialize(w)?;
        self.values_prefix.serialize(w)
    }
}

impl<V> BorshDeserialize for IterableMap<V> {
    fn deserialize_reader<R: io::Read>(r: &mut R) -> io::Result<Self> {
        Ok(Self { len: u32::deserialize_reader(r)?, keys_prefix: read_prefix(r)?, values_prefix: read_prefix(r)?, v: PhantomData })
    }
}
