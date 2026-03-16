use std::fs;
use std::path::Path;

use qrstatic::Grid;

const MAGIC: &[u8; 4] = b"QRSB";
const VERSION: u8 = 1;
const FLAG_PACKED_BITS: u8 = 0b0000_0001;

#[derive(Debug, Clone, PartialEq)]
pub struct BinaryContainer {
    pub width: usize,
    pub height: usize,
    pub n_frames: usize,
    pub seed: String,
    pub base_bias: f32,
    pub payload_bias_delta: f32,
    pub payload_len: usize,
    pub packed_bits: bool,
    pub frames: Vec<Grid<i8>>,
}

impl BinaryContainer {
    pub fn write_to_path(&self, path: &Path) -> Result<(), String> {
        let bytes = self.to_bytes()?;
        fs::write(path, bytes).map_err(|err| format!("failed to write {}: {err}", path.display()))
    }

    pub fn read_from_path(path: &Path) -> Result<Self, String> {
        let bytes =
            fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        Self::from_bytes(&bytes)
    }

    fn to_bytes(&self) -> Result<Vec<u8>, String> {
        if self.frames.len() != self.n_frames {
            return Err(format!(
                "container declares {} frames but has {}",
                self.n_frames,
                self.frames.len()
            ));
        }

        let width = u32::try_from(self.width).map_err(|_| "width does not fit in u32")?;
        let height = u32::try_from(self.height).map_err(|_| "height does not fit in u32")?;
        let n_frames = u32::try_from(self.n_frames).map_err(|_| "n_frames does not fit in u32")?;
        let payload_len =
            u32::try_from(self.payload_len).map_err(|_| "payload_len does not fit in u32")?;
        let seed_len =
            u16::try_from(self.seed.len()).map_err(|_| "seed length does not fit in u16")?;

        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        let flags = if self.packed_bits { FLAG_PACKED_BITS } else { 0 };
        out.push(flags);
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend_from_slice(&n_frames.to_le_bytes());
        out.extend_from_slice(&payload_len.to_le_bytes());
        out.extend_from_slice(&self.base_bias.to_le_bytes());
        out.extend_from_slice(&self.payload_bias_delta.to_le_bytes());
        out.extend_from_slice(&seed_len.to_le_bytes());
        out.extend_from_slice(self.seed.as_bytes());

        let mut frame_bits = Vec::new();
        for frame in &self.frames {
            if frame.width() != self.width || frame.height() != self.height {
                return Err("frame dimensions do not match container metadata".into());
            }
            for &cell in frame.data() {
                let encoded = match cell {
                    -1 => 0u8,
                    1 => 1u8,
                    other => {
                        return Err(format!(
                            "binary container only supports frame cells of -1 or 1, got {other}"
                        ));
                    }
                };
                if self.packed_bits {
                    frame_bits.push(encoded);
                } else {
                    out.push(encoded);
                }
            }
        }

        if self.packed_bits {
            out.extend(pack_bits(&frame_bits));
        }

        Ok(out)
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(bytes);
        let magic = cursor.read_array::<4>()?;
        if &magic != MAGIC {
            return Err("invalid magic for binary container".into());
        }

        let version = cursor.read_u8()?;
        if version != VERSION {
            return Err(format!("unsupported binary container version {version}"));
        }

        let flags = cursor.read_u8()?;
        let packed_bits = (flags & FLAG_PACKED_BITS) != 0;
        let width = cursor.read_u32()? as usize;
        let height = cursor.read_u32()? as usize;
        let n_frames = cursor.read_u32()? as usize;
        let payload_len = cursor.read_u32()? as usize;
        let base_bias = cursor.read_f32()?;
        let payload_bias_delta = cursor.read_f32()?;
        let seed_len = cursor.read_u16()? as usize;
        let seed_bytes = cursor.read_exact(seed_len)?;
        let seed = String::from_utf8(seed_bytes.to_vec())
            .map_err(|_| "seed is not valid UTF-8".to_string())?;

        let cells_per_frame = width
            .checked_mul(height)
            .ok_or_else(|| "frame dimensions overflow usize".to_string())?;
        let total_cells = cells_per_frame
            .checked_mul(n_frames)
            .ok_or_else(|| "frame count overflows usize".to_string())?;
        let frame_bytes = if packed_bits {
            let packed_len = total_cells.div_ceil(8);
            let packed = cursor.read_exact(packed_len)?;
            unpack_bits(packed, total_cells)
        } else {
            cursor.read_exact(total_cells)?.to_vec()
        };

        if !cursor.is_at_end() {
            return Err("trailing bytes after binary container payload".into());
        }

        let frames = frame_bytes
            .chunks_exact(cells_per_frame)
            .map(|chunk| {
                let data = chunk
                    .iter()
                    .map(|&byte| match byte {
                        0 => Ok(-1i8),
                        1 => Ok(1i8),
                        other => Err(format!("invalid binary frame cell byte {other}")),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Grid::from_vec(data, width, height))
            })
            .collect::<Result<Vec<_>, String>>()?;

        Ok(Self {
            width,
            height,
            n_frames,
            seed,
            base_bias,
            payload_bias_delta,
            payload_len,
            packed_bits,
            frames,
        })
    }
}

fn pack_bits(bits: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bits.len().div_ceil(8));
    let mut current = 0u8;
    for (idx, &bit) in bits.iter().enumerate() {
        if bit != 0 {
            current |= 1 << (idx % 8);
        }
        if idx % 8 == 7 {
            out.push(current);
            current = 0;
        }
    }
    if !bits.len().is_multiple_of(8) {
        out.push(current);
    }
    out
}

fn unpack_bits(bytes: &[u8], n_bits: usize) -> Vec<u8> {
    (0..n_bits)
        .map(|idx| (bytes[idx / 8] >> (idx % 8)) & 1)
        .collect()
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| "binary container offset overflow".to_string())?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| "binary container truncated".to_string())?;
        self.offset = end;
        Ok(slice)
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let slice = self.read_exact(N)?;
        slice
            .try_into()
            .map_err(|_| "failed to read fixed-size array".to_string())
    }

    fn read_u8(&mut self) -> Result<u8, String> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.read_array::<2>()?))
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.read_array::<4>()?))
    }

    fn read_f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.read_array::<4>()?))
    }

    fn is_at_end(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::BinaryContainer;
    use qrstatic::Grid;

    #[test]
    fn container_roundtrip_preserves_frames_and_metadata() {
        let container = BinaryContainer {
            width: 2,
            height: 2,
            n_frames: 2,
            seed: "test-seed".into(),
            base_bias: 0.8,
            payload_bias_delta: 0.1,
            payload_len: 11,
            packed_bits: false,
            frames: vec![
                Grid::from_vec(vec![1, -1, -1, 1], 2, 2),
                Grid::from_vec(vec![-1, 1, 1, -1], 2, 2),
            ],
        };

        let bytes = container.to_bytes().unwrap();
        let decoded = BinaryContainer::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, container);
    }

    #[test]
    fn rejects_invalid_cell_bytes() {
        let mut bytes = BinaryContainer {
            width: 1,
            height: 1,
            n_frames: 1,
            seed: "seed".into(),
            base_bias: 0.8,
            payload_bias_delta: 0.1,
            payload_len: 0,
            packed_bits: false,
            frames: vec![Grid::from_vec(vec![1], 1, 1)],
        }
        .to_bytes()
        .unwrap();
        *bytes.last_mut().unwrap() = 9;
        assert!(BinaryContainer::from_bytes(&bytes).is_err());
    }

    #[test]
    fn packed_container_roundtrip_preserves_frames_and_metadata() {
        let container = BinaryContainer {
            width: 3,
            height: 3,
            n_frames: 2,
            seed: "packed-seed".into(),
            base_bias: 0.8,
            payload_bias_delta: 0.1,
            payload_len: 2,
            packed_bits: true,
            frames: vec![
                Grid::from_vec(vec![1, -1, 1, -1, 1, -1, 1, -1, 1], 3, 3),
                Grid::from_vec(vec![-1, 1, -1, 1, -1, 1, -1, 1, -1], 3, 3),
            ],
        };

        let bytes = container.to_bytes().unwrap();
        let decoded = BinaryContainer::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, container);
    }
}
