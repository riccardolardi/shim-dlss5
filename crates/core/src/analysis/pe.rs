//! Just enough PE parsing for us: the COFF machine type (bitness), the section
//! table, and reading named sections, so string scans touch only the parts of
//! a 200 MB executable that can hold import names or a version resource.
//!
//! Hand-rolled on purpose: the format is stable, the subset is tiny, and it
//! keeps a large dependency out of the binary.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::model::Bitness;

const MACHINE_I386: u16 = 0x14C;
const MACHINE_AMD64: u16 = 0x8664;
const MACHINE_ARM64: u16 = 0xAA64;
/// Enough for the DOS stub, PE header and a section table of 96 entries.
const HEADER_READ: usize = 8192;
/// Hard cap for one section read; sections beyond this are truncated.
const MAX_SECTION_READ: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub offset: u64,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeInfo {
    pub machine: u16,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeError {
    NotPe(&'static str),
    Io(String),
}

impl std::fmt::Display for PeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotPe(why) => write!(f, "not a PE file: {why}"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl PeInfo {
    pub fn bitness(&self) -> Option<Bitness> {
        match self.machine {
            MACHINE_I386 => Some(Bitness::X86),
            MACHINE_AMD64 | MACHINE_ARM64 => Some(Bitness::X64),
            _ => None,
        }
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }
}

pub fn read_header(path: &Path) -> Result<PeInfo, PeError> {
    let mut file = File::open(path).map_err(|e| PeError::Io(e.to_string()))?;
    let mut buf = vec![0u8; HEADER_READ];
    let n = read_fully(&mut file, &mut buf).map_err(|e| PeError::Io(e.to_string()))?;
    parse_header(&buf[..n])
}

pub fn parse_header(bytes: &[u8]) -> Result<PeInfo, PeError> {
    if bytes.len() < 0x40 || &bytes[..2] != b"MZ" {
        return Err(PeError::NotPe("missing MZ signature"));
    }
    let pe_off = u32_at(bytes, 0x3C).ok_or(PeError::NotPe("truncated DOS header"))? as usize;
    if bytes.len() < pe_off + 24 || &bytes[pe_off..pe_off + 4] != b"PE\0\0" {
        return Err(PeError::NotPe("missing PE signature"));
    }
    let coff = pe_off + 4;
    let machine = u16_at(bytes, coff).ok_or(PeError::NotPe("truncated COFF header"))?;
    let count = u16_at(bytes, coff + 2).ok_or(PeError::NotPe("truncated COFF header"))? as usize;
    let opt_size =
        u16_at(bytes, coff + 16).ok_or(PeError::NotPe("truncated COFF header"))? as usize;
    let table = coff + 20 + opt_size;

    let mut sections = Vec::with_capacity(count);
    for i in 0..count {
        let at = table + i * 40;
        let Some(row) = bytes.get(at..at + 40) else {
            return Err(PeError::NotPe("section table runs past the header"));
        };
        let name_end = row[..8].iter().position(|&b| b == 0).unwrap_or(8);
        sections.push(Section {
            name: String::from_utf8_lossy(&row[..name_end]).into_owned(),
            size: u32::from_le_bytes([row[16], row[17], row[18], row[19]]) as u64,
            offset: u32::from_le_bytes([row[20], row[21], row[22], row[23]]) as u64,
        });
    }
    Ok(PeInfo { machine, sections })
}

/// Raw bytes of one section (truncated to the file and to `MAX_SECTION_READ`).
pub fn read_section(path: &Path, section: &Section) -> Result<Vec<u8>, PeError> {
    let mut file = File::open(path).map_err(|e| PeError::Io(e.to_string()))?;
    let len = file
        .metadata()
        .map_err(|e| PeError::Io(e.to_string()))?
        .len();
    if section.offset >= len {
        return Ok(Vec::new());
    }
    let size = section.size.min(len - section.offset).min(MAX_SECTION_READ) as usize;
    file.seek(SeekFrom::Start(section.offset))
        .map_err(|e| PeError::Io(e.to_string()))?;
    let mut buf = vec![0u8; size];
    let n = read_fully(&mut file, &mut buf).map_err(|e| PeError::Io(e.to_string()))?;
    buf.truncate(n);
    Ok(buf)
}

/// Bytes of every section whose name is listed, concatenated. Falls back to
/// the whole file (capped) when none of them exist, e.g. for packed or
/// unusually named sections.
pub fn read_sections_or_file(path: &Path, info: &PeInfo, names: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for name in names {
        if let Some(section) = info.section(name) {
            match read_section(path, section) {
                Ok(bytes) => out.extend(bytes),
                Err(e) => tracing::debug!(file = %path.display(), %name, %e, "section unreadable"),
            }
        }
    }
    if out.is_empty() {
        let whole = Section {
            name: "*".into(),
            offset: 0,
            size: MAX_SECTION_READ,
        };
        out = read_section(path, &whole).unwrap_or_default();
    }
    out
}

fn read_fully(file: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        let n = file.read(&mut buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    Ok(total)
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(at)?,
        *b.get(at + 1)?,
        *b.get(at + 2)?,
        *b.get(at + 3)?,
    ]))
}

/// Builds tiny but structurally valid PE files for tests and fixtures.
#[cfg(test)]
pub mod testing {
    use super::*;

    pub const X86: u16 = MACHINE_I386;
    pub const X64: u16 = MACHINE_AMD64;

    /// A PE with the given machine type and sections `(name, contents)`.
    /// The optional header is zero-filled (we never read it).
    pub fn build_pe(machine: u16, sections: &[(&str, &[u8])]) -> Vec<u8> {
        const PE_OFF: usize = 0x80;
        const OPT_SIZE: usize = 0xF0; // PE32+ optional header size
        let table = PE_OFF + 4 + 20 + OPT_SIZE;
        let headers_end = table + sections.len() * 40;
        let mut data_off = align(headers_end, 0x200);

        let mut out = vec![0u8; headers_end];
        out[..2].copy_from_slice(b"MZ");
        out[0x3C..0x40].copy_from_slice(&(PE_OFF as u32).to_le_bytes());
        out[PE_OFF..PE_OFF + 4].copy_from_slice(b"PE\0\0");
        let coff = PE_OFF + 4;
        out[coff..coff + 2].copy_from_slice(&machine.to_le_bytes());
        out[coff + 2..coff + 4].copy_from_slice(&(sections.len() as u16).to_le_bytes());
        out[coff + 16..coff + 18].copy_from_slice(&(OPT_SIZE as u16).to_le_bytes());

        let mut bodies: Vec<(usize, &[u8])> = Vec::new();
        for (i, (name, bytes)) in sections.iter().enumerate() {
            let row = table + i * 40;
            let n = name.len().min(8);
            out[row..row + n].copy_from_slice(&name.as_bytes()[..n]);
            let size = align(bytes.len().max(1), 0x200);
            out[row + 8..row + 12].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
            out[row + 16..row + 20].copy_from_slice(&(size as u32).to_le_bytes());
            out[row + 20..row + 24].copy_from_slice(&(data_off as u32).to_le_bytes());
            bodies.push((data_off, bytes));
            data_off += size;
        }
        out.resize(data_off, 0);
        for (off, bytes) in bodies {
            out[off..off + bytes.len()].copy_from_slice(bytes);
        }
        out
    }

    fn align(n: usize, to: usize) -> usize {
        n.div_ceil(to) * to
    }

    /// Write a 64-bit exe whose `.rdata` holds the given strings.
    pub fn write_exe(path: &Path, machine: u16, rdata: &[&str]) {
        let mut body = Vec::new();
        for s in rdata {
            body.extend_from_slice(s.as_bytes());
            body.push(0);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(
            path,
            build_pe(machine, &[(".text", b"\xC3"), (".rdata", &body)]),
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    #[test]
    fn parses_machine_and_sections_from_a_built_pe() {
        let pe = build_pe(X64, &[(".text", b"\xC3"), (".rdata", b"d3d12.dll\0")]);
        let info = parse_header(&pe).unwrap();
        assert_eq!(info.bitness(), Some(Bitness::X64));
        assert_eq!(info.sections.len(), 2);
        let rdata = info.section(".rdata").unwrap();
        assert_eq!(rdata.size, 0x200);
        assert_eq!(
            &pe[rdata.offset as usize..rdata.offset as usize + 9],
            b"d3d12.dll"
        );

        let pe32 = build_pe(X86, &[]);
        assert_eq!(parse_header(&pe32).unwrap().bitness(), Some(Bitness::X86));
    }

    #[test]
    fn rejects_non_pe_input() {
        assert!(matches!(parse_header(b"hello"), Err(PeError::NotPe(_))));
        let mut bad = build_pe(X64, &[]);
        bad[0x80] = b'X';
        assert!(matches!(parse_header(&bad), Err(PeError::NotPe(_))));
        let mut short = build_pe(X64, &[(".a", b"1"), (".b", b"2")]);
        short.truncate(0x80 + 24 + 0xF0 + 40);
        assert!(parse_header(&short).is_err());
    }

    #[test]
    fn reads_sections_from_disk_and_falls_back_to_whole_file() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("g.exe");
        write_exe(&exe, X64, &["vulkan-1.dll", "D3D12CreateDevice"]);
        let info = read_header(&exe).unwrap();
        let bytes = read_sections_or_file(&exe, &info, &[".rdata", ".idata"]);
        assert!(bytes.windows(12).any(|w| w == b"vulkan-1.dll"));
        assert!(!bytes.starts_with(b"MZ"));

        let fallback = read_sections_or_file(&exe, &info, &[".nothing"]);
        assert!(fallback.starts_with(b"MZ"));
        assert_eq!(
            fallback.len() as u64,
            std::fs::metadata(&exe).unwrap().len()
        );
    }

    #[test]
    fn section_past_end_of_file_is_empty_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("g.exe");
        std::fs::write(&exe, build_pe(X64, &[])).unwrap();
        let s = Section {
            name: ".x".into(),
            offset: 1 << 40,
            size: 10,
        };
        assert!(read_section(&exe, &s).unwrap().is_empty());
    }
}
