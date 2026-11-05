//! Network throughput from the kernel's 64-bit interface counters. No packets are sent.

use std::time::Instant;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    pub down_bytes_per_sec: f64,
    pub up_bytes_per_sec: f64,
}

/// Bytes in and out since boot over physical-ish interfaces: up, not loopback, and not
/// point-to-point (VPN tunnels would count the same traffic twice).
fn totals() -> Option<(u64, u64)> {
    let mut mib = [libc::CTL_NET, libc::PF_ROUTE, 0, 0, libc::NET_RT_IFLIST2, 0];
    let mut size = 0usize;
    // SAFETY: the first call sizes the buffer; the second fills at most `size` bytes.
    let buffer = unsafe {
        if libc::sysctl(
            mib.as_mut_ptr(),
            6,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        let mut buffer = vec![0u8; size];
        if libc::sysctl(
            mib.as_mut_ptr(),
            6,
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        buffer.truncate(size);
        buffer
    };
    let skip = libc::IFF_LOOPBACK | libc::IFF_POINTOPOINT;
    let header = std::mem::size_of::<libc::if_msghdr2>();
    let (mut down, mut up) = (0u64, 0u64);
    let mut offset = 0;
    while let Some(rest) = buffer.get(offset..) {
        if rest.len() < 4 {
            break;
        }
        let length = usize::from(u16::from_ne_bytes([rest[0], rest[1]]));
        if length == 0 {
            break;
        }
        if i32::from(rest[3]) == libc::RTM_IFINFO2 && rest.len() >= header {
            // SAFETY: at least one full header is in the slice; read unaligned from bytes.
            let message: libc::if_msghdr2 =
                unsafe { std::ptr::read_unaligned(rest.as_ptr().cast()) };
            if message.ifm_flags & libc::IFF_UP != 0 && message.ifm_flags & skip == 0 {
                down += message.ifm_data.ifi_ibytes;
                up += message.ifm_data.ifi_obytes;
            }
        }
        offset += length;
    }
    Some((down, up))
}

#[derive(Default)]
pub struct NetworkSampler {
    previous: Option<(Instant, u64, u64)>,
}

impl NetworkSampler {
    pub fn sample(&mut self) -> Option<Network> {
        let (down, up) = totals()?;
        let now = Instant::now();
        let (then, last_down, last_up) = self.previous.replace((now, down, up))?;
        let secs = now.duration_since(then).as_secs_f64();
        Some(Network {
            down_bytes_per_sec: crate::rate(last_down, down, secs),
            up_bytes_per_sec: crate::rate(last_up, up, secs),
        })
    }
}
