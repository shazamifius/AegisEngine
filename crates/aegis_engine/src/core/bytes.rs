//! Module d'accès mémoire sécurisé en Rust pur (remplace la crate `bytemuck`).

use std::mem::size_of;
use std::slice;

/// Convertit une référence vers une structure `T: Copy` alignée `#[repr(C)]` en tranche d'octets `&[u8]`.
#[inline]
pub fn as_bytes<T: Copy>(val: &T) -> &[u8] {
    let ptr = val as *const T as *const u8;
    unsafe { slice::from_raw_parts(ptr, size_of::<T>()) }
}

/// Convertit une tranche de structures `&[T]` alignées `#[repr(C)]` en tranche d'octets `&[u8]`.
#[inline]
pub fn cast_slice<T: Copy>(slice: &[T]) -> &[u8] {
    let ptr = slice.as_ptr() as *const u8;
    unsafe { slice::from_raw_parts(ptr, slice.len() * size_of::<T>()) }
}
