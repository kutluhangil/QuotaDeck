//! Read-only access to the System Management Controller (AppleSMC).
//!
//! The SMC holds the Mac's temperature, fan and power sensors as four-character keys
//! (`Tp05`, `F0Ac`, `PSTR`, ...). Reading needs no privilege; this module never writes.

use std::ffi::c_void;

use crate::iokit::{self, IoObject, KernReturn};

/// A four-character SMC key, e.g. `*b"F0Ac"`.
pub type Key = [u8; 4];

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Vers {
    major: u8,
    minor: u8,
    build: u8,
    reserved: u8,
    release: u16,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct PLimit {
    version: u16,
    length: u16,
    cpu: u32,
    gpu: u32,
    mem: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct KeyInfo {
    size: u32,
    kind: u32,
    attributes: u8,
}

/// Mirrors the kernel's `SMCParamStruct`; the layout must stay exactly 80 bytes.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Param {
    key: u32,
    vers: Vers,
    plimit: PLimit,
    info: KeyInfo,
    result: u8,
    status: u8,
    command: u8,
    index: u32,
    bytes: [u8; 32],
}

const HANDLE_EVENT: u32 = 2;
const CMD_READ: u8 = 5;
const CMD_KEY_AT_INDEX: u8 = 8;
const CMD_KEY_INFO: u8 = 9;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceOpen(service: IoObject, task: u32, kind: u32, conn: *mut IoObject) -> KernReturn;
    fn IOServiceClose(conn: IoObject) -> KernReturn;
    fn IOConnectCallStructMethod(
        conn: IoObject,
        selector: u32,
        input: *const c_void,
        input_size: usize,
        output: *mut c_void,
        output_size: *mut usize,
    ) -> KernReturn;
}

/// A decoded SMC value with its type tag (`flt `, `ui8 `, `sp78`, ...).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub kind: Key,
    pub value: f64,
}

/// A key with its type and size already known.
#[derive(Clone, Copy)]
pub struct Probe {
    pub key: Key,
    info: KeyInfo,
}

/// An open connection to AppleSMC. Closed on drop.
pub struct Smc {
    conn: IoObject,
}

impl Smc {
    /// Opens AppleSMC, preferring the `AppleSMCKeysEndpoint` service Apple Silicon exposes.
    pub fn open() -> Option<Self> {
        let services = iokit::services(c"AppleSMC");
        let chosen = services
            .iter()
            .find(|service| service.name() == "AppleSMCKeysEndpoint")
            .or(services.first())?;
        let mut conn: IoObject = 0;
        // SAFETY: `chosen` is a live service; a successful open yields a connection closed on drop.
        let status = unsafe { IOServiceOpen(chosen.0, iokit::task(), 0, &mut conn) };
        (status == 0).then(|| Self { conn })
    }

    fn call(&self, input: &Param) -> Option<Param> {
        let mut output = Param::default();
        let mut size = std::mem::size_of::<Param>();
        // SAFETY: both buffers are valid `Param`s of the size passed.
        let status = unsafe {
            IOConnectCallStructMethod(
                self.conn,
                HANDLE_EVENT,
                (input as *const Param).cast(),
                std::mem::size_of::<Param>(),
                (&mut output as *mut Param).cast(),
                &mut size,
            )
        };
        (status == 0 && output.result == 0).then_some(output)
    }

    /// Number of keys the SMC publishes (`#KEY`).
    pub fn key_count(&self) -> Option<u32> {
        let reading = self.read_raw(*b"#KEY")?;
        Some(u32::from_be_bytes([
            reading.1[0],
            reading.1[1],
            reading.1[2],
            reading.1[3],
        ]))
    }

    /// The key at `index` in the SMC's own order.
    pub fn key_at(&self, index: u32) -> Option<Key> {
        let output = self.call(&Param {
            command: CMD_KEY_AT_INDEX,
            index,
            ..Param::default()
        })?;
        Some(output.key.to_be_bytes())
    }

    /// Every key the SMC publishes. Roughly two thousand on Apple Silicon; enumerate once.
    pub fn keys(&self) -> Vec<Key> {
        let count = self.key_count().unwrap_or(0);
        (0..count).filter_map(|index| self.key_at(index)).collect()
    }

    fn read_raw(&self, key: Key) -> Option<(KeyInfo, [u8; 32])> {
        let probe = self.probe(key)?;
        Some((probe.info, self.read_bytes(&probe)?))
    }

    fn read_bytes(&self, probe: &Probe) -> Option<[u8; 32]> {
        let output = self.call(&Param {
            key: u32::from_be_bytes(probe.key),
            command: CMD_READ,
            info: KeyInfo {
                size: probe.info.size,
                ..KeyInfo::default()
            },
            ..Param::default()
        })?;
        Some(output.bytes)
    }

    /// Looks a key's type and size up once, so later reads take one call instead of two.
    pub fn probe(&self, key: Key) -> Option<Probe> {
        let code = u32::from_be_bytes(key);
        let info = self
            .call(&Param {
                key: code,
                command: CMD_KEY_INFO,
                ..Param::default()
            })?
            .info;
        Some(Probe { key, info })
    }

    /// Reads a key whose type was looked up with [`Smc::probe`].
    pub fn read_probe(&self, probe: &Probe) -> Option<Reading> {
        let bytes = self.read_bytes(probe)?;
        let kind = probe.info.kind.to_be_bytes();
        decode(kind, probe.info.size as usize, &bytes).map(|value| Reading { kind, value })
    }

    /// Reads and decodes one key. `None` when the key is absent or its type is not numeric.
    pub fn read(&self, key: Key) -> Option<Reading> {
        self.read_probe(&self.probe(key)?)
    }
}

impl Drop for Smc {
    fn drop(&mut self) {
        // SAFETY: `conn` came from a successful IOServiceOpen and is closed exactly once.
        unsafe {
            IOServiceClose(self.conn);
        }
    }
}

/// Decodes the numeric SMC types seen on Apple Silicon and older Macs.
pub fn decode(kind: Key, size: usize, bytes: &[u8]) -> Option<f64> {
    let b = |i: usize| bytes.get(i).copied();
    match &kind {
        b"flt " if size == 4 => Some(f32::from_le_bytes([b(0)?, b(1)?, b(2)?, b(3)?]) as f64),
        b"ui8 " => Some(b(0)? as f64),
        b"ui16" => Some(u16::from_be_bytes([b(0)?, b(1)?]) as f64),
        b"ui32" => Some(u32::from_be_bytes([b(0)?, b(1)?, b(2)?, b(3)?]) as f64),
        b"sp78" => Some(i16::from_be_bytes([b(0)?, b(1)?]) as f64 / 256.0),
        b"fpe2" => Some(u16::from_be_bytes([b(0)?, b(1)?]) as f64 / 4.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_matches_the_kernel_layout() {
        assert_eq!(std::mem::size_of::<Param>(), 80);
    }

    #[test]
    fn decodes_the_types_apple_silicon_uses() {
        assert_eq!(decode(*b"flt ", 4, &999.0f32.to_le_bytes()), Some(999.0));
        assert_eq!(decode(*b"ui8 ", 1, &[5]), Some(5.0));
        assert_eq!(decode(*b"ui16", 2, &[0x96, 0x00]), Some(38400.0));
        assert_eq!(decode(*b"sp78", 2, &[0x2c, 0x80]), Some(44.5));
        assert_eq!(decode(*b"fpe2", 2, &[0x0f, 0xa0]), Some(1000.0));
    }

    #[test]
    fn refuses_unknown_or_short_values() {
        assert_eq!(decode(*b"ch8*", 4, &[1, 2, 3, 4]), None);
        assert_eq!(decode(*b"flt ", 2, &[0, 0]), None);
        assert_eq!(decode(*b"ui32", 4, &[1, 2]), None);
    }
}
