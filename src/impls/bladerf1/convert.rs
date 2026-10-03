use crate::{Capability, Error};
use libbladerf_rs::bladerf1::SampleFormat;
use libbladerf_rs::Buffer;
use num_complex::Complex32;

const INV_2048: f32 = 1.0 / 2048.0;
const INV_128: f32 = 1.0 / 128.0;

fn convert_sc16q11_to_complex32(src: &[u8], dst: &mut [Complex32]) -> usize {
    let len = (src.len() / 4).min(dst.len());
    for (chunk, out) in src.as_chunks::<4>().0.iter().take(len).zip(dst.iter_mut()) {
        let i_val = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 * INV_2048;
        let q_val = i16::from_le_bytes([chunk[2], chunk[3]]) as f32 * INV_2048;
        *out = Complex32::new(i_val, q_val);
    }
    len
}

fn convert_sc8q7_to_complex32(src: &[u8], dst: &mut [Complex32]) -> usize {
    let len = (src.len() / 2).min(dst.len());
    for (chunk, out) in src.as_chunks::<2>().0.iter().take(len).zip(dst.iter_mut()) {
        let i_val = (chunk[0] as i8) as f32 * INV_128;
        let q_val = (chunk[1] as i8) as f32 * INV_128;
        *out = Complex32::new(i_val, q_val);
    }
    len
}

fn convert_sc16q11_packed_to_complex32(src: &[u8], dst: &mut [Complex32]) -> usize {
    let groups = src.len() / 6;
    let num_samples = (groups * 2).min(dst.len());
    for i in 0..num_samples / 2 {
        let si = 6 * i;
        let w0 = u16::from_le_bytes([src[si], src[si + 1]]);
        let w1 = u16::from_le_bytes([src[si + 2], src[si + 3]]);
        let w2 = u16::from_le_bytes([src[si + 4], src[si + 5]]);
        let i0 = sign_extend_12(w0 & 0x0FFF) * INV_2048;
        let q0 = sign_extend_12((w0 >> 12) | ((w1 & 0x00FF) << 4)) * INV_2048;
        let i1 = sign_extend_12((w1 >> 8) | ((w2 & 0x000F) << 8)) * INV_2048;
        let q1 = sign_extend_12(w2 >> 4) * INV_2048;
        dst[i * 2] = Complex32::new(i0, q0);
        dst[i * 2 + 1] = Complex32::new(i1, q1);
    }
    num_samples
}

#[inline(always)]
const fn sign_extend_12(val: u16) -> f32 {
    ((val << 4) as i16 >> 4) as f32
}

pub(super) fn convert_bytes_to_complex32(
    format: SampleFormat,
    src: &[u8],
    dst: &mut [Complex32],
) -> Result<usize, Error> {
    let written = match format {
        SampleFormat::Sc16Q11 => convert_sc16q11_to_complex32(src, dst),
        SampleFormat::Sc8Q7 => convert_sc8q7_to_complex32(src, dst),
        SampleFormat::Sc16Q11Packed => convert_sc16q11_packed_to_complex32(src, dst),
        _ => {
            return Err(Error::unsupported_reason(
                Capability::RxStreaming,
                format!("unsupported sample format: {format:?}"),
            ));
        }
    };
    Ok(written)
}

fn convert_complex32_to_sc16q11(src: &[Complex32], dst: &mut [u8]) -> usize {
    let len = src.len().min(dst.len() / 4);
    for (s, chunk) in src
        .iter()
        .take(len)
        .zip(dst.as_chunks_mut::<4>().0.iter_mut())
    {
        let i_val = (s.re * 2048.0).clamp(-2048.0, 2047.999) as i16;
        let q_val = (s.im * 2048.0).clamp(-2048.0, 2047.999) as i16;
        chunk[..2].copy_from_slice(&i_val.to_le_bytes());
        chunk[2..].copy_from_slice(&q_val.to_le_bytes());
    }
    len
}

fn convert_complex32_to_sc8q7(src: &[Complex32], dst: &mut [u8]) -> usize {
    let len = src.len().min(dst.len() / 2);
    for (s, chunk) in src
        .iter()
        .take(len)
        .zip(dst.as_chunks_mut::<2>().0.iter_mut())
    {
        let i_val = (s.re * 128.0).clamp(-128.0, 127.999) as i8;
        let q_val = (s.im * 128.0).clamp(-128.0, 127.999) as i8;
        chunk[0] = i_val as u8;
        chunk[1] = q_val as u8;
    }
    len
}

pub(super) fn convert_complex32_to_bytes(
    format: SampleFormat,
    src: &[Complex32],
    dst: &mut [u8],
) -> Result<usize, Error> {
    let written = match format {
        SampleFormat::Sc16Q11 => convert_complex32_to_sc16q11(src, dst),
        SampleFormat::Sc8Q7 => convert_complex32_to_sc8q7(src, dst),
        _ => {
            return Err(Error::unsupported_reason(
                Capability::TxStreaming,
                format!("unsupported TX sample format: {format:?}"),
            ));
        }
    };
    Ok(written)
}

/// Converts DMA buffers into `Complex32` and carries partially consumed
/// buffers over to the next `read` call.
pub(super) struct RxConverter {
    format: SampleFormat,
    pending: Option<(Buffer, usize)>,
}

impl RxConverter {
    pub(super) fn new(format: SampleFormat) -> Self {
        Self {
            format,
            pending: None,
        }
    }

    pub(super) fn take_pending(&mut self) -> Option<Buffer> {
        self.pending.take().map(|(buffer, _)| buffer)
    }

    /// Copies samples left over from a previous buffer into `out`.
    ///
    /// Returns the number of samples written and, if the buffer is now fully
    /// consumed, the buffer for recycling.
    pub(super) fn drain_pending(
        &mut self,
        out: &mut [Complex32],
    ) -> Result<(usize, Option<Buffer>), Error> {
        let Some((buf, offset)) = self.pending.take() else {
            return Ok((0, None));
        };
        let (written, next_offset) = self.convert_from(&buf, offset, out)?;
        if next_offset < buf.len() {
            self.pending = Some((buf, next_offset));
            Ok((written, None))
        } else {
            Ok((written, Some(buf)))
        }
    }

    /// Converts a freshly received buffer into `out`.
    ///
    /// Returns the number of samples written and, if the buffer was fully
    /// consumed, the buffer for recycling. Otherwise the remainder is kept
    /// for the next `drain_pending`.
    pub(super) fn consume(
        &mut self,
        buf: Buffer,
        out: &mut [Complex32],
    ) -> Result<(usize, Option<Buffer>), Error> {
        let (written, next_offset) = self.convert_from(&buf, 0, out)?;
        if next_offset < buf.len() {
            self.pending = Some((buf, next_offset));
            Ok((written, None))
        } else {
            Ok((written, Some(buf)))
        }
    }

    fn convert_from(
        &self,
        buf: &Buffer,
        offset: usize,
        out: &mut [Complex32],
    ) -> Result<(usize, usize), Error> {
        let bytes_per_sample = self.format.sample_size();
        let samples_available = (buf.len() - offset) / bytes_per_sample;
        let samples = out.len().min(samples_available);
        convert_bytes_to_complex32(
            self.format,
            &buf[offset..offset + samples * bytes_per_sample],
            &mut out[..samples],
        )?;
        Ok((samples, offset + samples * bytes_per_sample))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rx_restart_returns_the_partial_buffer_without_replaying_old_samples() {
        let mut converter = RxConverter::new(SampleFormat::Sc16Q11);
        let mut buffer = Buffer::new(16);
        buffer.extend_from_slice(&[1u8; 16]);
        let mut out = [Complex32::default(); 1];
        assert!(converter.consume(buffer, &mut out).unwrap().1.is_none());
        assert_eq!(&*converter.take_pending().unwrap(), &[1u8; 16]);
        assert!(converter.take_pending().is_none());
        assert_eq!(converter.drain_pending(&mut out).unwrap().0, 0);
    }

    #[test]
    fn sc16q11_round_trip_full_scale() {
        let samples = [
            Complex32::new(0.5, -0.5),
            Complex32::new(1.0, -1.0),
            Complex32::new(0.0, 0.25),
        ];
        let mut bytes = [0u8; 12];
        assert_eq!(
            convert_complex32_to_bytes(SampleFormat::Sc16Q11, &samples, &mut bytes).unwrap(),
            3
        );
        let mut back = [Complex32::default(); 3];
        assert_eq!(
            convert_bytes_to_complex32(SampleFormat::Sc16Q11, &bytes, &mut back).unwrap(),
            3
        );
        for (a, b) in samples.iter().zip(back.iter()) {
            assert!((a.re - b.re).abs() < 1e-3, "{a} vs {b}");
            assert!((a.im - b.im).abs() < 1e-3, "{a} vs {b}");
        }
    }

    #[test]
    fn sc8q7_round_trip() {
        let samples = [Complex32::new(0.5, -0.25)];
        let mut bytes = [0u8; 2];
        convert_complex32_to_bytes(SampleFormat::Sc8Q7, &samples, &mut bytes).unwrap();
        let mut back = [Complex32::default(); 1];
        convert_bytes_to_complex32(SampleFormat::Sc8Q7, &bytes, &mut back).unwrap();
        assert!((back[0].re - 0.5).abs() < 1e-2);
        assert!((back[0].im + 0.25).abs() < 1e-2);
    }

    #[test]
    fn packed_matches_unpacked() {
        let unpacked: Vec<u8> = [1024i16, -1024, 2047, -2048]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let mut packed = [0u8; 6];
        SampleFormat::pack_sc16q11_packed(&unpacked, &mut packed, 2).unwrap();
        let mut from_packed = [Complex32::default(); 2];
        let mut from_unpacked = [Complex32::default(); 2];
        convert_bytes_to_complex32(SampleFormat::Sc16Q11Packed, &packed, &mut from_packed).unwrap();
        convert_bytes_to_complex32(SampleFormat::Sc16Q11, &unpacked, &mut from_unpacked).unwrap();
        assert_eq!(from_packed, from_unpacked);
    }

    #[test]
    fn rx_converter_carries_over_partial_buffers() {
        let mut converter = RxConverter::new(SampleFormat::Sc16Q11);
        let mut buf = Buffer::new(16);
        buf.extend_from_slice(&[0u8; 16]);
        let mut out = [Complex32::default(); 3];
        let (written, recycled) = converter.consume(buf, &mut out).unwrap();
        assert_eq!(written, 3);
        assert!(recycled.is_none());
        let (written, recycled) = converter.drain_pending(&mut out).unwrap();
        assert_eq!(written, 1);
        assert!(recycled.is_some());
        let (written, recycled) = converter.drain_pending(&mut out).unwrap();
        assert_eq!((written, recycled.is_none()), (0, true));
    }
}
